"""Exercise modules/nixos/beszel-setup.py against a fake PocketBase hub."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer

SCRIPT = Path(__file__).parent.parent / "modules" / "nixos" / "beszel-setup.py"
EMAIL = "mirsella@mirsella.mooo.com"


class FakeHub(BaseHTTPRequestHandler):
    state = {}
    calls = []

    def log_message(self, *args):
        pass

    def _json(self, status, payload):
        body = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _read_body(self):
        length = int(self.headers.get("Content-Length", 0))
        return json.loads(self.rfile.read(length) or b"{}")

    def do_GET(self):
        if self.path == "/api/health":
            return self._json(200, {"message": "ok"})
        if self.path.startswith("/api/settings"):
            return self._json(200, self.state["settings"])
        collection = self.path.split("/")[3]
        records = self.state[collection]
        return self._json(200, {"totalItems": len(records), "items": records})

    def do_POST(self):
        body = self._read_body()
        if self.path.endswith("/auth-with-password"):
            if body.get("password") != "correct-horse":
                return self._json(400, {"message": "Failed to authenticate."})
            return self._json(200, {"token": "test-token"})
        collection = self.path.split("/")[3]
        self.calls.append(("POST", collection))
        record = {"id": f"{collection}-1", **body}
        self.state[collection].append(record)
        return self._json(200, record)

    def do_PATCH(self):
        body = self._read_body()
        if self.path == "/api/settings":
            self.calls.append(("PATCH", "settings"))
            for section, values in body.items():
                self.state["settings"][section].update(values)
            return self._json(204, {})
        collection = self.path.split("/")[3]
        self.calls.append(("PATCH", collection))
        record = next(
            r for r in self.state[collection] if r["id"] == self.path.rsplit("/", 1)[1]
        )
        record.update(body)
        return self._json(200, record)


def fresh_state():
    return {
        "settings": {
            "smtp": {
                "enabled": False,
                "host": "",
                "port": 25,
                "username": "",
                "authMethod": "PLAIN",
                "tls": False,
                "localName": "",
            },
            "meta": {"senderName": "", "senderAddress": "support@example.com"},
        },
        "users": [],
        "systems": [],
    }


class BeszelSetup(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = HTTPServer(("127.0.0.1", 0), FakeHub)
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()
        cls.hub = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="beszel-setup-")
        self.addCleanup(self.tmp.cleanup)
        FakeHub.state = fresh_state()
        FakeHub.calls = []
        self.pw = Path(self.tmp.name, "superuser")
        self.pw.write_text("correct-horse\n")
        self.key = Path(self.tmp.name, "resend")
        self.key.write_text("re_testkey\n")
        self.env = dict(os.environ, BESZEL_HUB=self.hub)

    def run_script(self, password="correct-horse"):
        if password != "correct-horse":
            self.pw.write_text(password + "\n")
        return subprocess.run(
            [sys.executable, str(SCRIPT), str(self.pw), str(self.key)],
            capture_output=True,
            text=True,
            env=self.env,
        )

    def test_converges_empty_hub_then_noops(self):
        first = self.run_script()
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertIn("settings patched", first.stdout)
        self.assertIn("hub user created", first.stdout)
        self.assertIn("system created", first.stdout)
        smtp = FakeHub.state["settings"]["smtp"]
        self.assertEqual(
            (
                smtp["enabled"],
                smtp["host"],
                smtp["port"],
                smtp["username"],
                smtp["tls"],
            ),
            (True, "smtp.resend.com", 465, "resend", True),
        )
        meta = FakeHub.state["settings"]["meta"]
        self.assertEqual(
            (meta["senderName"], meta["senderAddress"]),
            ("Beszel", "noreply@voxride.com"),
        )
        user = FakeHub.state["users"][0]
        self.assertEqual((user["email"], user["role"]), (EMAIL, "admin"))
        self.assertEqual(FakeHub.state["systems"][0]["users"], ["users-1"])

        FakeHub.calls = []
        second = self.run_script()
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertIn("settings already converged", second.stdout)
        self.assertIn("hub user exists", second.stdout)
        self.assertIn("system exists", second.stdout)
        self.assertEqual(FakeHub.calls, [])

    def test_bad_superuser_password_hints_bootstrap(self):
        proc = self.run_script(password="wrong")
        self.assertNotEqual(proc.returncode, 0)
        self.assertIn("superuser upsert", proc.stderr)

    def test_repairs_existing_records_without_resetting_password(self):
        self.assertEqual(self.run_script().returncode, 0)
        FakeHub.state["users"][0].update(role="user", password="changed-by-user")
        FakeHub.state["systems"][0].update(port="9999", users=[])
        FakeHub.calls = []

        proc = self.run_script()
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(FakeHub.calls, [("PATCH", "users"), ("PATCH", "systems")])
        self.assertEqual(FakeHub.state["users"][0]["role"], "admin")
        self.assertEqual(FakeHub.state["users"][0]["password"], "changed-by-user")
        self.assertEqual(FakeHub.state["systems"][0]["port"], "45876")
        self.assertEqual(FakeHub.state["systems"][0]["users"], ["users-1"])


if __name__ == "__main__":
    unittest.main()
