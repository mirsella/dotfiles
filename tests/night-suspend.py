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
        self.connection_filter = ""
        self.maintenance = ""
        self.worker_status = 1
        self.pools = "fast\ntank\n"
        self.pool_health = {"fast": True, "tank": True}
        self.pool_status = {"fast": "", "tank": ""}
        self.clear_effective = True
        self.clears = []
        self.storage_faults = ""
        self.calls = []

        def output(*args):
            if args[0] == "loginctl":
                return json.dumps(self.sessions)
            if args[0] == "ss":
                self.connection_filter = args[-1]
                return self.connections
            if args[0] == "systemctl":
                return self.maintenance
            if args[0] == "zpool" and args[1] == "list":
                return self.pools
            if args[0] == "zpool" and args[1] == "status" and "-x" in args:
                name = args[-1]
                if self.pool_health[name]:
                    return f"pool '{name}' is healthy\n"
                return f"  pool: {name}\n state: ONLINE\n"
            if args[0] == "zpool" and args[1] == "status":
                return self.pool_status[args[-1]]
            if args[0] == "zpool" and args[1] == "clear":
                self.clears.append(args[-1])
                self.pool_health[args[-1]] = self.clear_effective
                return ""
            if args[0] == "journalctl":
                return self.storage_faults
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

    def pool_status_text(self, state="ONLINE", read=0, write=0, cksum=0, scan=None):
        text = "  pool: tank\n state: ONLINE\n"
        if scan:
            text += f"  scan: {scan}\n"
        return text + (
            "config:\n\n"
            "\tNAME        STATE     READ WRITE CKSUM\n"
            "\ttank        ONLINE       0     0     0\n"
            f"\t  mirror-0  {state}       {read}     {write}     {cksum}\n"
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

    def test_fresh_rotated_log_blocks_without_decompressing(self):
        (self.logs / "access-rotated.log.gz").write_bytes(b"compressed log")
        self.assertEqual(night.blocker(time.time()), "web request within 30 minutes")

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
        self.assertIn("not dst 127.0.0.0/8", self.connection_filter)
        self.assertIn("not dst ::1/128", self.connection_filter)

    def test_backup_and_build_workers_block(self):
        self.maintenance = "db-backup.service loaded active running Backup\n"
        self.assertIn("maintenance", night.blocker(time.time()))
        self.maintenance = ""
        self.worker_status = 0
        self.assertEqual(night.blocker(time.time()), "Nix build workers")

    def test_corrected_pool_errors_are_cleared_and_suspend_proceeds(self):
        self.pool_health["tank"] = False
        self.pool_status["tank"] = self.pool_status_text(cksum=4)
        night.main()
        self.assertEqual(self.clears, ["tank"])
        self.assertEqual(
            self.calls[-1], ("systemctl", "--check-inhibitors=yes", "suspend")
        )

    def test_read_write_errors_block_without_clearing(self):
        self.pool_health["tank"] = False
        self.pool_status["tank"] = self.pool_status_text(write=3)
        self.assertEqual(
            night.blocker(time.time()), "ZFS tank has read or write errors"
        )
        self.assertEqual(self.clears, [])

    def test_offline_vdev_blocks_without_clearing(self):
        self.pool_health["tank"] = False
        self.pool_status["tank"] = self.pool_status_text(state="FAULTED")
        self.assertEqual(
            night.blocker(time.time()), "ZFS tank vdevs not online: FAULTED"
        )
        self.assertEqual(self.clears, [])

    def test_recent_storage_faults_skip_clearing(self):
        self.pool_health["tank"] = False
        self.pool_status["tank"] = self.pool_status_text(cksum=4)
        self.storage_faults = "uas_eh_abort_handler tag 8\n"
        self.assertEqual(
            night.blocker(time.time()), "recent storage faults, not clearing tank"
        )
        self.assertEqual(self.clears, [])

    def test_ineffective_clear_blocks(self):
        self.pool_health["tank"] = False
        self.pool_status["tank"] = self.pool_status_text(cksum=4)
        self.clear_effective = False
        self.assertEqual(
            night.blocker(time.time()), "ZFS tank still unhealthy after zpool clear"
        )
        self.assertEqual(self.clears, ["tank"])

    def test_scan_in_progress_blocks(self):
        self.pool_health["tank"] = False
        self.pool_status["tank"] = self.pool_status_text(
            scan="resilver in progress since Tue Sep 29 09:14:33 2026"
        )
        self.assertEqual(night.blocker(time.time()), "ZFS tank scan in progress")
        self.assertEqual(self.clears, [])

    def test_missing_pool_blocks(self):
        self.pools = "fast\n"
        self.assertEqual(night.blocker(time.time()), "missing ZFS pools: tank")

    def test_malformed_recent_log_fails_closed(self):
        (self.logs / "access-invalid.log").write_text("not JSON\n")
        with self.assertRaises(json.JSONDecodeError):
            night.blocker(time.time())


if __name__ == "__main__":
    unittest.main()
