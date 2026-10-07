"""Guard the one-time installer's config edits and rollback cleanup."""

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
