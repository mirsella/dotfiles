"""The overnight check must never suspend a server with recent activity."""

import importlib.util
import json
from datetime import datetime
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "night_suspend",
    Path(__file__).resolve().parents[1] / "modules/nixos/night-suspend.py",
)
night = importlib.util.module_from_spec(spec)
spec.loader.exec_module(night)


class MidnightCheck(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        self.logs = root / "caddy"
        self.logs.mkdir()
        self.enterContext(patch.object(night, "LOGS", self.logs))
        clock = self.enterContext(patch.object(night, "datetime"))
        clock.now.return_value = datetime(2026, 9, 24, 0, 1)
        self.sessions = [{"class": "manager"}]
        self.connections = ""
        self.maintenance = ""
        self.worker_status = 1
        self.calls = []

        def output(*args):
            if args[0] == "loginctl":
                return json.dumps(self.sessions)
            if args[0] == "ss":
                return self.connections
            if args[0] == "systemctl":
                return self.maintenance
            raise AssertionError(args)

        def run(args, **kwargs):
            self.calls.append(args)
            if args[0] == "pgrep":
                return subprocess.CompletedProcess(args, self.worker_status, stderr=b"")
            if args[0] == "rtcwake":
                return subprocess.CompletedProcess(args, 0)
            if args[:2] == ("systemctl", "--check-inhibitors=yes"):
                return subprocess.CompletedProcess(args, 0)
            raise AssertionError(args)

        self.enterContext(patch.object(night, "output", output))
        self.enterContext(patch.object(night.subprocess, "run", run))

    def request(self, seconds_ago):
        path = self.logs / "access-mirsella.mooo.com.log"
        path.write_text(
            json.dumps({"ts": time.time() - seconds_ago, "request": {}}) + "\n"
        )

    def test_idle_server_suspends_and_programs_morning_wake(self):
        self.request(31 * 60)
        night.main()
        self.assertEqual(
            self.calls[-1], ("systemctl", "--check-inhibitors=yes", "suspend")
        )
        self.assertEqual(self.calls[-2][:3], ("rtcwake", "--mode=no", "--time"))
        self.assertEqual(datetime.fromtimestamp(int(self.calls[-2][3])).hour, 7)

    def test_recent_request_blocks_after_connection_closed(self):
        self.request(29 * 60)
        night.main()
        self.assertEqual(self.calls, [])

    def test_late_timer_after_morning_wake_does_not_suspend(self):
        night.datetime.now.return_value = datetime(2026, 9, 24, 7, 0)
        night.main()
        self.assertEqual(self.calls, [])

    def test_ssh_or_tty_blocks(self):
        self.sessions = [{"class": "user", "tty": "tty1"}]
        self.assertEqual(night.blocker(time.time()), "SSH or local login session")

    def test_live_web_connection_blocks(self):
        self.connections = "0 0 192.168.1.19:443 192.168.1.61:55222"
        self.assertEqual(night.blocker(time.time()), "active SSH or web connection")

    def test_backup_and_build_workers_block(self):
        self.maintenance = "db-backup.service loaded active running Backup\n"
        self.assertIn("maintenance", night.blocker(time.time()))
        self.maintenance = ""
        self.worker_status = 0
        self.assertEqual(night.blocker(time.time()), "Nix build workers")

    def test_malformed_recent_log_fails_closed(self):
        (self.logs / "access-invalid.log").write_text("not JSON\n")
        with self.assertRaises(json.JSONDecodeError):
            night.blocker(time.time())


if __name__ == "__main__":
    unittest.main()
