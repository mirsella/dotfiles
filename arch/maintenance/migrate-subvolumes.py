#!/usr/bin/env python3
"""One-time migration of Arch /nix and Docker off the Timeshift root subvolume."""

import os
from pathlib import Path
import re
import subprocess
import tempfile


FSTAB = Path("/etc/fstab")
TARGETS = [
    (Path("/nix"), "@nix", ["nix-daemon.socket", "nix-daemon.service"]),
    (Path("/var/lib/docker"), "@docker", ["docker.socket", "docker.service"]),
]


def run(*args: str) -> None:
    subprocess.run(args, check=True)


def output(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def write_fstab(text: str) -> None:
    with tempfile.NamedTemporaryFile(mode="w", dir=FSTAB.parent, delete=False) as file:
        file.write(text)
        temporary = Path(file.name)
    try:
        os.chmod(temporary, FSTAB.stat().st_mode & 0o777)
        os.replace(temporary, FSTAB)
    finally:
        temporary.unlink(missing_ok=True)


def main() -> None:
    if os.geteuid() != 0:
        raise PermissionError("Run with sudo")
    if output("findmnt", "-n", "-o", "FSTYPE", "/") != "btrfs":
        raise RuntimeError("Root must be Btrfs")
    uuid = output("findmnt", "-n", "-o", "UUID", "/")
    mount_options = ",".join(
        option
        for option in output("findmnt", "-n", "-o", "OPTIONS", "/").split(",")
        if not option.startswith(("subvol=", "subvolid="))
    )
    old_fstab = FSTAB.read_text()
    for target, name, _ in TARGETS:
        if output("findmnt", "-n", "-o", "TARGET", "-T", str(target)) != "/":
            raise RuntimeError(f"{target} must still be in the root subvolume")
        if re.search(rf"^\s*\S+\s+{re.escape(str(target))}\s+", old_fstab, re.M):
            raise RuntimeError(f"{target} already has an fstab entry")
        if (target.parent / (target.name + ".before-subvolume")).exists():
            raise RuntimeError(
                f"Backup already exists beside {target}; inspect before retrying"
            )

    with tempfile.TemporaryDirectory(prefix="btrfs-maintenance-", dir="/run") as top:
        run("mount", "-t", "btrfs", "-o", "subvolid=5", f"UUID={uuid}", top)
        try:
            for _, name, _ in TARGETS:
                if (Path(top) / name).exists():
                    raise RuntimeError(
                        f"{name} already exists; inspect before retrying"
                    )
            # Stop writers before copying, then keep the old trees for rollback.
            mounted = []
            moved = []
            created = []
            active_units = [
                unit
                for _, _, units in TARGETS
                for unit in units
                if subprocess.run(
                    ["systemctl", "is-active", "--quiet", unit]
                ).returncode
                == 0
            ]
            try:
                for _, _, units in reversed(TARGETS):
                    run("systemctl", "stop", *units)
                for target, name, _ in TARGETS:
                    run("btrfs", "subvolume", "create", str(Path(top) / name))
                    created.append(Path(top) / name)
                    run(
                        "rsync",
                        "-aHAX",
                        "--numeric-ids",
                        f"{target}/",
                        f"{top}/{name}/",
                    )
                for target, _, _ in TARGETS:
                    backup = target.with_name(target.name + ".before-subvolume")
                    target.rename(backup)
                    moved.append((target, backup))
                    target.mkdir(mode=0o755)
                additions = "".join(
                    f"UUID={uuid} {target} btrfs {mount_options},subvol=/{name} 0 0\n"
                    for target, name, _ in TARGETS
                )
                write_fstab(
                    old_fstab.rstrip()
                    + "\n\n# Keep changing stores out of Timeshift root snapshots\n"
                    + additions
                )
                run("systemctl", "daemon-reload")
                for target, _, _ in TARGETS:
                    run("mount", str(target))
                    mounted.append(target)
                for target, name, _ in TARGETS:
                    if (
                        output("findmnt", "-n", "-o", "FSROOT", str(target))
                        != "/" + name
                    ):
                        raise RuntimeError(f"{target} mounted the wrong subvolume")
            except Exception:
                for target in reversed(mounted):
                    run("umount", str(target))
                write_fstab(old_fstab)
                run("systemctl", "daemon-reload")
                for target, backup in reversed(moved):
                    if target.exists():
                        target.rmdir()
                    backup.rename(target)
                for subvol in reversed(created):
                    run("btrfs", "subvolume", "delete", str(subvol))
                raise
            finally:
                if active_units:
                    run("systemctl", "start", *active_units)
        finally:
            run("umount", top)

    print("Migrated /nix and /var/lib/docker to separate subvolumes.")
    print(
        "After reboot and verification, remove /nix.before-subvolume and /var/lib/docker.before-subvolume."
    )


if __name__ == "__main__":
    main()
