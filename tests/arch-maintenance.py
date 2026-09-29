"""Guard the on-disk config syntax and Btrfs alert boundary."""

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).resolve().parents[1] / "arch/maintenance"


def load(name: str, filename: str):
    spec = importlib.util.spec_from_file_location(name, SOURCE / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


installer = load("arch_apply", "apply.py")
space = load("btrfs_space", "btrfs-space-check.py")
rollback = load("arch_rollback", "cleanup-rollback.py")


class ArchConfig(unittest.TestCase):
    def test_nix_settings_update_once_without_discarding_other_settings(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "nix.conf"
            path.write_text(
                "# local keys\nextra-substituters = https://cache\nmin-free = 1\nmin-free=2\n"
            )
            self.assertTrue(
                installer.set_values(path, {"min-free": "10737418240"}, " = ")
            )
            self.assertEqual(
                path.read_text(),
                "# local keys\nextra-substituters = https://cache\nmin-free = 10737418240\n",
            )
            self.assertFalse(
                installer.set_values(path, {"min-free": "10737418240"}, " = ")
            )

    def test_shell_style_settings_have_no_spaces_around_equals(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "autosnap.conf"
            path.write_text("deleteSnapshots=true\nmaxSnapshots = 3\n")
            installer.set_values(path, {"maxSnapshots": "2"}, "=")
            self.assertEqual(path.read_text(), "deleteSnapshots=true\nmaxSnapshots=2\n")


class BtrfsHeadroom(unittest.TestCase):
    def test_alerts_on_aggregate_metadata_or_low_unallocated_space(self):
        with (
            patch.object(space.subprocess, "check_output") as usage,
            patch.object(space.subprocess, "run") as notify,
        ):
            usage.return_value = (
                "Device unallocated: 8589934592\n"
                "Metadata,DUP: Size:100, Used:90\n"
                "Metadata,single: Size:100, Used:89\n"
            )
            space.check("/", notify=True)
            notify.assert_not_called()

            usage.return_value = usage.return_value.replace("Used:89", "Used:90")
            with self.assertRaisesRegex(RuntimeError, "90.0% used"):
                space.check("/", notify=True)
            notify.assert_called_once()

            notify.reset_mock()
            usage.return_value = usage.return_value.replace(
                "Device unallocated: 8589934592", "Device unallocated: 7516192768"
            ).replace("Used:90", "Used:80")
            with self.assertRaisesRegex(RuntimeError, "7.00 GiB unallocated"):
                space.check("/", notify=True)
            notify.assert_called_once()

    def test_unrecognized_output_fails_instead_of_reporting_healthy(self):
        with patch.object(
            space.subprocess, "check_output", return_value="Device unallocated: 0\n"
        ):
            with self.assertRaisesRegex(ValueError, "Unexpected btrfs"):
                space.check("/")

    def test_notification_failure_keeps_the_metadata_alert(self):
        with (
            patch.object(
                space.subprocess,
                "check_output",
                return_value=(
                    "Device unallocated: 8589934592\nMetadata,DUP: Size:100, Used:90\n"
                ),
            ),
            patch.object(
                space.subprocess,
                "run",
                side_effect=subprocess.CalledProcessError(1, "notify-send"),
            ),
        ):
            with self.assertRaisesRegex(
                RuntimeError, "90.0% used.*Desktop notification failed"
            ):
                space.check("/", notify=True)

    def test_reclaim_is_bounded_and_stops_at_target(self):
        gib = space.GIB
        with (
            patch.object(
                space,
                "usage",
                side_effect=[(54, 5 * gib), (54, 9 * gib), (54, 12 * gib)],
            ),
            patch.object(space.subprocess, "run") as balance,
        ):
            space.reclaim("/")
            self.assertEqual(balance.call_count, 2)
            balance.assert_called_with(
                ["btrfs", "balance", "start", "-dusage=75,limit=5", "/"], check=True
            )

    def test_reclaim_skips_balance_when_headroom_is_healthy(self):
        with (
            patch.object(space, "usage", return_value=(54, 12 * space.GIB)),
            patch.object(space.subprocess, "run") as balance,
        ):
            space.reclaim("/")
            balance.assert_not_called()

    def test_reclaim_fails_when_limited_balance_makes_no_progress(self):
        gib = space.GIB
        with (
            patch.object(space, "usage", side_effect=[(54, 5 * gib), (54, 5 * gib)]),
            patch.object(space.subprocess, "run") as balance,
        ):
            with self.assertRaisesRegex(RuntimeError, "did not free device space"):
                space.reclaim("/")
            balance.assert_called_once()


class RollbackCleanup(unittest.TestCase):
    def test_checks_both_stores_before_deleting_either_copy(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory) / name for name in ("nix", "docker")]
            for path in paths:
                path.mkdir()
                (path / "sentinel").write_text("keep")
            stores = (
                (Path("/nix"), "/@nix", paths[0]),
                (Path("/var/lib/docker"), "/@docker", paths[1]),
            )

            def output(*args):
                if args[:4] == ("findmnt", "-n", "-o", "UUID"):
                    return "same-uuid"
                if args[:4] == ("findmnt", "-n", "-o", "FSROOT"):
                    return "/@nix" if args[4] == "/nix" else "/@docker"
                if args[:4] == ("findmnt", "-n", "-o", "TARGET"):
                    return "/"
                if args[0] == "docker":
                    return "/wrong/docker/root"
                raise AssertionError(args)

            result = subprocess.CompletedProcess([], 1, stdout="", stderr="")
            with (
                patch.object(rollback, "STORES", stores),
                patch.object(rollback, "output", side_effect=output) as probe,
                patch.object(rollback.subprocess, "run", return_value=result),
            ):
                with self.assertRaisesRegex(RuntimeError, "Docker uses"):
                    rollback.clean()
                for path in paths:
                    self.assertTrue((path / "sentinel").exists())
                probe.side_effect = lambda *args: (
                    "/var/lib/docker" if args[0] == "docker" else output(*args)
                )
                rollback.clean()
                for path in paths:
                    self.assertFalse(path.exists())


if __name__ == "__main__":
    unittest.main()
