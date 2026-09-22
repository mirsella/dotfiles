"""Test the evaluated tank-unlock script from stdin without accessing devices."""

import os
import subprocess
import sys
import unittest


source = sys.stdin.read()
mocks = r"""
set -eu
PATH=/nonexistent
opens=0
cryptsetup() {
  printf '%s\n' "$*"
  case "$1" in
    status) [[ "$ALREADY_OPEN" = 1 ]];;
    open) opens=$((opens + 1)); (( opens > FAIL_OPENS ));;
    *) return 99;;
  esac
}
udevadm() { printf 'udevadm %s\n' "$*"; return "$UDEV_STATUS"; }
sleep() { printf 'sleep %s\n' "$*"; }
"""


class TankUnlock(unittest.TestCase):
    def test_unlock_and_failure_paths(self):
        # name, already open, udev status, failed opens, exit, waits, opens, sleeps
        cases = [
            ("both disks ready", 0, 0, 0, 0, 2, 2, 0),
            ("already unlocked", 1, 0, 0, 0, 0, 0, 0),
            ("device timeout", 0, 42, 0, 42, 1, 0, 0),
            ("transient USB failures", 0, 0, 2, 0, 2, 4, 2),
            ("retry limit", 0, 0, 99, 1, 1, 12, 11),
        ]
        for name, active, udev, failures, status, waits, opens, sleeps in cases:
            with self.subTest(name=name):
                result = subprocess.run(
                    ["bash", "-s"],
                    input=mocks + source,
                    text=True,
                    capture_output=True,
                    timeout=5,
                    env=dict(
                        os.environ,
                        ALREADY_OPEN=str(active),
                        UDEV_STATUS=str(udev),
                        FAIL_OPENS=str(failures),
                    ),
                )
                self.assertEqual(result.returncode, status, result.stderr)
                calls = result.stdout.splitlines()
                for prefix, count in [
                    ("udevadm ", waits),
                    ("open ", opens),
                    ("sleep ", sleeps),
                ]:
                    self.assertEqual(
                        sum(line.startswith(prefix) for line in calls), count, calls
                    )


if __name__ == "__main__":
    unittest.main()
