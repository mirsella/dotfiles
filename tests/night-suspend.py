"""Exercise the generated suspend script against fake devices and system services."""

import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest


source = Path(sys.argv.pop(1)).read_text()
disks = list(dict.fromkeys(re.findall(r"(?:wwn|ata)-[A-Za-z0-9_-]+", source)))


class NightSuspend(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="night-suspend-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        for directory in ("bin", "devices", "block", "runtime"):
            (self.root / directory).mkdir()
        self.alarm = self.root / "wakealarm"
        self.alarm.write_text("unchanged")
        self.uptime = self.root / "uptime"
        self.uptime.write_text("10000.00 0.00\n")
        self.state = self.root / "runtime" / "idle"
        self.counters = " ".join(f"{disk}:100:200:0" for disk in disks)
        self.state.write_text(f"7000 {self.counters}\n")
        self.calls = self.root / "calls"
        self.env = dict(
            os.environ,
            RUNTIME_DIRECTORY=str(self.root / "runtime"),
            CALLS=str(self.calls),
            NOW="10000",
            HOUR="01",
            SESSIONS=json.dumps([{"class": "manager", "tty": None}]),
            CONNECTIONS="",
            BUSY_UNIT="",
            PGREP_STATUS="1",
            LIST_FAIL="0",
        )
        for index, disk in enumerate(disks):
            device = f"sd{index}"
            (self.root / "devices" / disk).symlink_to(device)
            (self.root / "block" / device).mkdir()
            (self.root / "block" / device / "stat").write_text(
                "0 0 100 0 0 0 200 0 0 0 0\n"
            )

        stubs = {
            "date": 'case "$*" in +%s) echo "$NOW";; +%H) echo "$HOUR";; *) echo 20000;; esac',
            "loginctl": 'printf "%s\\n" "$SESSIONS"',
            "ss": 'printf "%s\\n" "$CONNECTIONS"',
            "pgrep": 'exit "$PGREP_STATUS"',
            "systemctl": """
                printf '%s\\n' "$*" >> "$CALLS"
                case "$1" in
                    list-units)
                        [ "$LIST_FAIL" = 0 ] || exit 1
                        for unit in "${@:2}"; do
                            if [ "$unit" = "$BUSY_UNIT" ]; then
                                echo "$unit loaded activating start Maintenance"
                            fi
                        done
                        ;;
                    --check-inhibitors=yes) [ "$2" = suspend ];;
                    *) exit 2;;
                esac
            """,
        }
        for name, body in stubs.items():
            command = self.root / "bin" / name
            command.write_text("#!/usr/bin/env bash\nset -eu\n" + body + "\n")
            command.chmod(0o755)

        script = source.replace("/dev/disk/by-id", str(self.root / "devices")).replace(
            "/sys/block", str(self.root / "block")
        )
        script = script.replace("/sys/class/rtc/rtc0/wakealarm", str(self.alarm))
        script = script.replace("/proc/uptime", str(self.uptime))
        # Keep the actual generated runtime inputs, with command doubles first.
        script, replacements = re.subn(
            r"^(export PATH=)(.*)$", rf"\1{self.root}/bin:\2", script, flags=re.M
        )
        self.assertEqual(
            replacements, 1, "command doubles must replace the runtime PATH"
        )
        self.script = self.root / "check"
        self.script.write_text(script)

    def run_check(self, *, suspend=False, success=True):
        self.calls.unlink(missing_ok=True)
        result = subprocess.run(
            ["bash", self.script], env=self.env, capture_output=True, text=True
        )
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
        calls = self.calls.read_text().splitlines() if self.calls.exists() else []
        self.assertEqual("--check-inhibitors=yes suspend" in calls, suspend)
        if suspend:
            self.assertEqual(self.alarm.read_text(), "20000\n")
        elif self.alarm.is_file():
            self.assertEqual(self.alarm.read_text(), "unchanged")

    def test_idle_machine_with_only_lingering_manager_can_suspend(self):
        self.run_check(suspend=True)
        calls = self.calls.read_text().splitlines()
        self.assertEqual(sum(line.startswith("list-units ") for line in calls), 1)

    def test_first_run_after_boot_starts_a_new_idle_window(self):
        self.state.unlink()
        self.run_check()

    def test_busy_disks_skip_unnecessary_service_and_session_probes(self):
        self.state.unlink()
        self.env["SESSIONS"] = "invalid JSON"
        self.run_check()
        self.assertFalse(self.calls.exists())

    def test_disk_activity_resets_idle_window(self):
        (self.root / "block/sd0/stat").write_text("0 0 101 0 0 0 200 0 0 0 0\n")
        self.run_check()
        self.assertTrue(self.state.read_text().startswith("10000 "))
        self.uptime.write_text("12699.00 0.00\n")
        self.run_check()
        self.uptime.write_text("12700.00 0.00\n")
        self.run_check(suspend=True)

    def test_photo_ssd_activity_blocks_suspend_while_hdds_are_idle(self):
        index = next(i for i, disk in enumerate(disks) if disk.startswith("ata-"))
        (self.root / f"block/sd{index}/stat").write_text("0 0 101 0 0 0 200 0 0 0 0\n")
        self.run_check()

    def test_inflight_io_blocks_suspend_without_completed_transfers(self):
        (self.root / "block/sd0/stat").write_text("0 0 100 0 0 0 200 0 1 0 0\n")
        self.run_check()
        self.uptime.write_text("30000.00 0.00\n")
        self.run_check()
        (self.root / "block/sd0/stat").write_text("0 0 100 0 0 0 200 0 0 0 0\n")
        self.run_check()
        self.assertTrue(self.state.read_text().startswith("30000 "))

    def test_wall_clock_jump_does_not_shorten_idle_window(self):
        self.state.write_text(f"8500 {self.counters}\n")
        self.env["NOW"] = "864000"
        self.run_check()

    def test_invalid_idle_state_never_requests_suspend(self):
        self.state.write_text(f"invalid {self.counters}\n")
        self.run_check(success=False)

    def test_non_tty_ssh_session_blocks_suspend(self):
        self.env["SESSIONS"] = json.dumps([{"class": "user", "tty": None}])
        self.run_check()

    def test_service_connection_blocks_suspend(self):
        self.env["CONNECTIONS"] = "0 0 127.0.0.1:14096 127.0.0.1:55555"
        self.run_check()

    def test_running_maintenance_blocks_suspend(self):
        self.env["BUSY_UNIT"] = "zfs-scrub.service"
        self.run_check()

    def test_nix_build_blocks_suspend(self):
        self.env["PGREP_STATUS"] = "0"
        self.run_check()

    def test_failed_process_query_never_requests_suspend(self):
        self.env["PGREP_STATUS"] = "2"
        self.run_check(success=False)

    def test_missing_disk_blocks_suspend(self):
        (self.root / "devices" / disks[0]).unlink()
        self.run_check()

    def test_failed_rtc_write_never_requests_suspend(self):
        self.alarm.unlink()
        self.alarm.mkdir()
        self.run_check(success=False)

    def test_failed_session_query_never_requests_suspend(self):
        self.env["SESSIONS"] = "invalid JSON"
        self.run_check(success=False)

    def test_failed_service_query_never_requests_suspend(self):
        self.env["LIST_FAIL"] = "1"
        self.run_check(success=False)

    def test_daytime_updates_baseline_without_considering_suspend(self):
        self.env.update(HOUR="12", SESSIONS="invalid JSON")
        self.run_check()
        self.assertFalse(self.calls.exists())


if __name__ == "__main__":
    unittest.main()
