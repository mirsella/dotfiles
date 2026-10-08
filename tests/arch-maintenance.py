"""Guard the Arch maintenance installer's config edits."""

import importlib.util
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "arch_apply", Path(__file__).resolve().parents[1] / "arch/maintenance/apply.py"
)
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


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


if __name__ == "__main__":
    unittest.main()
