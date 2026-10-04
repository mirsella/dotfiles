"""Check the evaluated data crypttab with systemd's real generator."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


crypttab = sys.stdin.read()
systemd = Path(sys.argv.pop(1))
generator = systemd / "lib/systemd/system-generators/systemd-cryptsetup-generator"


class DataUnlock(unittest.TestCase):
    def test_optional_parallel_unlocking(self):
        with tempfile.TemporaryDirectory(prefix="data-unlock-") as directory:
            root = Path(directory)
            table = root / "crypttab"
            table.write_text(crypttab)
            output = root / "units"
            output.mkdir()
            subprocess.run(
                [generator, str(output), str(output), str(output)],
                env=dict(
                    os.environ,
                    SYSTEMD_CRYPTTAB=str(table),
                    SYSTEMD_IN_INITRD="0",
                    SYSTEMD_PROC_CMDLINE="",
                ),
                check=True,
                capture_output=True,
                text=True,
            )
            rows = [line.split() for line in crypttab.splitlines() if line.strip()]
            self.assertEqual(
                [row[0] for row in rows],
                ["fast-crypt", "tank1-crypt", "tank2-crypt", "tank3-crypt"],
            )
            for name, device, keyfile, flags in rows:
                with self.subTest(name=name):
                    unit = subprocess.check_output(
                        [
                            systemd / "bin/systemd-escape",
                            "--template=systemd-cryptsetup@.service",
                            name,
                        ],
                        text=True,
                    ).strip()
                    contents = (output / unit).read_text()
                    self.assertTrue(
                        (output / "cryptsetup.target.wants" / unit).is_symlink()
                    )
                    self.assertFalse(
                        (output / "cryptsetup.target.requires" / unit).exists()
                    )
                    self.assertNotIn("Before=cryptsetup.target", contents)
                    self.assertIn("headless=true", contents)
                    self.assertIn(device, contents)
                    if name == "fast-crypt":
                        self.assertEqual(keyfile, "-")
                        self.assertIn("tpm2-device=auto", contents)
                        self.assertIn("discard", contents)
                    else:
                        self.assertEqual(
                            keyfile, "/etc/luks/" + name.removesuffix("-crypt") + ".key"
                        )
                        self.assertIn(keyfile, contents)
                    self.assertIn("x-systemd.device-timeout=30s", flags)


if __name__ == "__main__":
    unittest.main()
