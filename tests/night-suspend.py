"""The overnight check must never suspend a server with recent activity."""

import importlib.util
import json
from datetime import datetime
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "hosts/predator"))
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
        self.problems = {}
        self.clear_effective = True
        self.clears = []
        self.storage_faults = ""
        self.storage = []
        self.dd_timeout = False
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
            if args[0] == "zpool" and args[1:3] == ("status", "-xj"):
                return json.dumps({"pools": self.problems})
            if args[0] == "zpool" and args[1:3] == ("status", "-x"):
                name = args[-1]
                if name not in self.problems:
                    return f"pool '{name}' is healthy\n"
                return f"  pool: {name}\n state: ONLINE\n"
            if args[0] == "zpool" and args[1] == "clear":
                self.clears.append(args[-1])
                if self.clear_effective:
                    del self.problems[args[-1]]
                return ""
            if args[0] == "journalctl":
                return self.storage_faults
            if args[0] == "lsblk":
                return json.dumps({"blockdevices": self.storage})
            raise AssertionError(args)

        def run(args, **kwargs):
            self.calls.append(args)
            if args[0] == "pgrep":
                return subprocess.CompletedProcess(args, self.worker_status, stderr=b"")
            if args[0] == "dd":
                if self.dd_timeout:
                    raise subprocess.TimeoutExpired(args, 90)
                return subprocess.CompletedProcess(args, 0)
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

    def pool_status(
        self, state="ONLINE", read=0, write=0, cksum=0, scan=None, errors=0
    ):
        pool = {
            "name": "tank",
            "state": "ONLINE",
            "error_count": errors,
            "vdevs": {
                "raidz1-0": {
                    "state": state,
                    "read_errors": read,
                    "write_errors": write,
                    "checksum_errors": cksum,
                },
            },
        }
        if scan:
            pool["scan_stats"] = {"state": scan}
        return pool

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

    def test_replication_and_recovery_jobs_block(self):
        for unit in (
            "syncoid-data.service",
            "syncoid-ncdata.service",
            "recovery-backup.service",
        ):
            with self.subTest(unit=unit):
                self.maintenance = f"{unit} loaded active running Backup\n"
                self.assertIn("maintenance", night.blocker(time.time()))
        self.assertIn("syncoid-*.service", night.MAINTENANCE)
        self.assertIn("recovery-backup.service", night.MAINTENANCE)

    def test_corrected_pool_errors_are_cleared_and_suspend_proceeds(self):
        self.problems["tank"] = self.pool_status(cksum=4)
        night.main()
        self.assertEqual(self.clears, ["tank"])
        self.assertEqual(
            self.calls[-1], ("systemctl", "--check-inhibitors=yes", "suspend")
        )

    def test_read_write_errors_block_without_clearing(self):
        self.problems["tank"] = self.pool_status(write=3)
        self.assertEqual(
            night.blocker(time.time()), "ZFS tank has read or write errors"
        )
        self.assertEqual(self.clears, [])

    def test_offline_vdev_blocks_without_clearing(self):
        self.problems["tank"] = self.pool_status(state="FAULTED")
        self.assertEqual(
            night.blocker(time.time()), "ZFS tank vdevs not online: FAULTED"
        )
        self.assertEqual(self.clears, [])

    def test_recent_storage_faults_skip_clearing(self):
        self.problems["tank"] = self.pool_status(cksum=4)
        self.storage_faults = "uas_eh_abort_handler tag 8\n"
        self.assertEqual(
            night.blocker(time.time()), "recent storage faults, not clearing tank"
        )
        self.assertEqual(self.clears, [])

    def test_ineffective_clear_blocks(self):
        self.problems["tank"] = self.pool_status(cksum=4)
        self.clear_effective = False
        self.assertEqual(
            night.blocker(time.time()), "ZFS tank still unhealthy after zpool clear"
        )
        self.assertEqual(self.clears, ["tank"])

    def test_scan_in_progress_blocks(self):
        self.problems["tank"] = self.pool_status(scan="SCANNING")
        self.assertEqual(night.blocker(time.time()), "ZFS tank scan in progress")
        self.assertEqual(self.clears, [])

    def test_missing_pool_blocks(self):
        self.pools = "fast\n"
        self.assertEqual(night.blocker(time.time()), "missing ZFS pools: tank")

    def test_permanent_errors_are_never_cleared(self):
        self.problems["tank"] = self.pool_status(cksum=4, errors=1)
        self.assertEqual(
            night.blocker(time.time()), "ZFS tank has permanent data errors"
        )
        self.assertEqual(self.clears, [])

    def test_other_pool_warnings_are_not_hidden_by_clearing(self):
        self.problems["tank"] = self.pool_status()
        self.assertEqual(night.blocker(time.time()), "ZFS tank requires attention")
        self.assertEqual(self.clears, [])

    def test_shared_storage_fault_pattern_blocks_former_hub_port(self):
        self.problems["tank"] = self.pool_status(cksum=4)
        self.storage_faults = "usb 2-3: USB disconnect, device number 4\n"
        self.assertEqual(
            night.blocker(time.time()), "recent storage faults, not clearing tank"
        )
        self.assertEqual(self.clears, [])

    def test_malformed_recent_log_fails_closed(self):
        (self.logs / "access-invalid.log").write_text("not JSON\n")
        with self.assertRaises(json.JSONDecodeError):
            night.blocker(time.time())

    def test_usb_disks_warmed_before_suspend(self):
        self.storage = [
            {"name": "/dev/sda", "tran": "sata"},
            {"name": "/dev/loop0", "tran": None},
            {"name": "/dev/sdc", "tran": "usb"},
            {"name": "/dev/sdd", "tran": "usb"},
        ]
        self.request(31 * 60)
        night.main()
        dds = [call for call in self.calls if call[0] == "dd"]
        self.assertEqual([call[1] for call in dds], ["if=/dev/sdc", "if=/dev/sdd"])
        self.assertEqual(
            self.calls[-1], ("systemctl", "--check-inhibitors=yes", "suspend")
        )

    def test_warm_up_timeout_blocks_suspend(self):
        self.storage = [{"name": "/dev/sdc", "tran": "usb"}]
        self.dd_timeout = True
        self.request(31 * 60)
        with self.assertRaises(subprocess.TimeoutExpired):
            night.main()
        self.assertEqual([call[0] for call in self.calls], ["pgrep", "dd"])


if __name__ == "__main__":
    unittest.main()
