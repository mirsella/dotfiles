"""Exercise evaluated backup scripts without databases, disks or private keys."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


scripts = json.load(sys.stdin)
MOUNTPOINT = "/srv/backup/recovery"
FAKE = """#!/usr/bin/env python3
import json, os
from pathlib import Path
import sys

name = Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["CALLS"], "a") as log:
    log.write(json.dumps([name, *args]) + "\\n")
if name == os.environ.get("FAIL"):
    sys.exit(9)
if name == "findmnt":
    sys.exit(0 if os.environ["MOUNT_OK"] == "1" else 1)
elif name == "install":
    directory = Path(args[-1])
    directory.mkdir(parents=True, exist_ok=True)
    directory.chmod(int(args[args.index("-m") + 1], 8))
elif name == "runuser":
    command = args[args.index("--") + 1:]
    assert Path(command[0]).name == "pg_dump"
    assert command[1:] == ["--format=custom", "nextcloud"]
    print("nextcloud database dump")
elif name == "podman":
    print(name + " database dump")
elif name == "tar":
    filename = next(arg.removeprefix("--file=") for arg in args if arg.startswith("--file="))
    Path(filename).write_bytes(b"archive fixture")
elif name == "cryptsetup":
    if args[0] == "luksHeaderBackup":
        header = Path(args[args.index("--header-backup-file") + 1])
        header.write_bytes(b"encrypted header fixture")
        header.chmod(0o400)
    elif args[0] == "luksUUID":
        assert Path(args[1]).is_file()
        print("uuid:" + args[1])
    else:
        raise AssertionError(args)
else:
    raise AssertionError(name)
"""


class Backups(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="backup-test-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.mount = self.root / "recovery"
        binaries = self.root / "bin"
        binaries.mkdir()
        fake = binaries / "fake"
        fake.write_text(FAKE)
        fake.chmod(0o755)
        for name in (
            "findmnt",
            "install",
            "runuser",
            "podman",
            "tar",
            "cryptsetup",
        ):
            (binaries / name).symlink_to(fake.name)
        self.calls = self.root / "calls"
        self.env = dict(
            os.environ,
            PATH=f"{binaries}:{os.environ['PATH']}",
            CALLS=str(self.calls),
            MOUNT_OK="1",
        )

    def run_job(self, name):
        return subprocess.run(
            ["bash", "-c", scripts[name].replace(MOUNTPOINT, str(self.mount))],
            env=self.env,
            preexec_fn=lambda: os.umask(0o077),
            capture_output=True,
            text=True,
        )

    def test_both_jobs_publish_private_complete_bundles(self):
        for name, out in (
            ("db-backup", self.mount / "db"),
            ("recovery-backup", self.mount),
        ):
            with self.subTest(job=name):
                result = self.run_job(name)
                self.assertEqual(result.returncode, 0, result.stderr)
                bundle = (out / "latest").resolve(strict=True)
                self.assertEqual(bundle.parent, out)
                self.assertEqual(bundle.stat().st_mode & 0o777, 0o700)
                self.assertEqual(list(out.glob(".incomplete.*")), [])
                self.assertFalse((out / ".latest").exists())
                self.assertTrue(
                    all(path.stat().st_mode & 0o077 == 0 for path in bundle.rglob("*"))
                )
                if name == "db-backup":
                    self.assertEqual(
                        sorted(path.name for path in bundle.iterdir()),
                        ["immich.dump", "nextcloud-config.tar", "nextcloud.dump"],
                    )
                else:
                    self.assertEqual(
                        sorted(
                            path.stem for path in (bundle / "luks").glob("*.header")
                        ),
                        ["fast", "root", "tank1", "tank2", "tank3"],
                    )
                    self.assertEqual(len(list((bundle / "luks").glob("*.uuid"))), 5)
                    self.assertTrue((bundle / "system-secrets.tar").is_file())
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertTrue(
            all(
                "tank/backup/recovery" in call for call in calls if call[0] == "findmnt"
            )
        )
        secrets = next(
            call for call in calls if call[0] == "tar" and "--directory=/" in call
        )
        self.assertTrue(
            {"etc/luks", "var/lib/sbctl", "etc/ssh/ssh_host_ed25519_key"}.issubset(
                secrets
            )
        )

    def test_missing_mount_never_creates_backup_directories(self):
        self.env["MOUNT_OK"] = "0"
        for name in scripts:
            with self.subTest(job=name):
                result = self.run_job(name)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Backups require", result.stderr)
                self.assertFalse(self.mount.exists())

    def test_partial_failure_preserves_last_complete_bundle(self):
        for name, out, failure in (
            ("db-backup", self.mount / "db", "podman"),
            ("recovery-backup", self.mount, "tar"),
        ):
            with self.subTest(job=name):
                out.mkdir(parents=True, exist_ok=True)
                old = out / "backup-20250101T000000.000000000Z"
                old.mkdir()
                (out / "latest").symlink_to(old.name)
                self.env["FAIL"] = failure
                result = self.run_job(name)
                self.assertEqual(result.returncode, 9, result.stderr)
                self.assertEqual((out / "latest").resolve(), old)
                self.assertEqual(list(out.glob("backup-*")), [old])
                self.assertEqual(list(out.glob(".incomplete.*")), [])

    def test_retention_removes_only_old_complete_bundles(self):
        for name, out in (
            ("db-backup", self.mount / "db"),
            ("recovery-backup", self.mount),
        ):
            with self.subTest(job=name):
                out.mkdir(parents=True, exist_ok=True)
                for day in range(1, 16):
                    (out / f"backup-202501{day:02}T000000.000000000Z").mkdir()
                protected = out / "backup-operator-notes"
                protected.write_text("keep")
                result = self.run_job(name)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(len(list(out.glob("backup-*/"))), 14)
                self.assertFalse((out / "backup-20250101T000000.000000000Z").exists())
                self.assertFalse((out / "backup-20250102T000000.000000000Z").exists())
                self.assertTrue((out / "latest").resolve(strict=True).is_dir())
                self.assertEqual(protected.read_text(), "keep")

    def test_clock_rollback_keeps_the_newly_published_bundle(self):
        for name, out in (
            ("db-backup", self.mount / "db"),
            ("recovery-backup", self.mount),
        ):
            with self.subTest(job=name):
                out.mkdir(parents=True, exist_ok=True)
                for day in range(1, 15):
                    (out / f"backup-999901{day:02}T000000.000000000Z").mkdir()
                result = self.run_job(name)
                self.assertEqual(result.returncode, 0, result.stderr)
                bundle = (out / "latest").resolve(strict=True)
                self.assertFalse(bundle.name.startswith("backup-9999"))
                self.assertEqual(len(list(out.glob("backup-*/"))), 14)


if __name__ == "__main__":
    unittest.main()
