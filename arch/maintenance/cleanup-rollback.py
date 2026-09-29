#!/usr/bin/env python3
"""Remove migration rollback copies only after the new stores work on reboot."""

import os
from pathlib import Path
import shutil
import subprocess


STORES = (
    (Path("/nix"), "/@nix", Path("/nix.before-subvolume")),
    (Path("/var/lib/docker"), "/@docker", Path("/var/lib/docker.before-subvolume")),
)


def output(*args: str) -> str:
    return subprocess.check_output(args, text=True, timeout=30).strip()


def clean() -> None:
    root_uuid = output("findmnt", "-n", "-o", "UUID", "/")
    backups = []
    for mount, expected_root, backup in STORES:
        if output("findmnt", "-n", "-o", "FSROOT", str(mount)) != expected_root:
            raise RuntimeError(f"{mount} is not mounted from {expected_root}")
        if output("findmnt", "-n", "-o", "UUID", str(mount)) != root_uuid:
            raise RuntimeError(f"{mount} is not on the root Btrfs filesystem")
        if backup.is_symlink() or (backup.exists() and not backup.is_dir()):
            raise RuntimeError(f"Unexpected rollback path: {backup}")
        if backup.exists():
            if output("findmnt", "-n", "-o", "TARGET", "-T", str(backup)) != "/":
                raise RuntimeError(f"Rollback directory is mounted: {backup}")
            nested = subprocess.run(
                ["findmnt", "-rn", "-o", "TARGET", "--submounts", str(backup)],
                capture_output=True,
                text=True,
                check=False,
            )
            if nested.returncode not in (0, 1):
                raise RuntimeError(f"Could not check mounts below {backup}")
            if nested.stdout.strip():
                raise RuntimeError(f"Rollback directory contains mounts: {backup}")
            backups.append(backup)

    subprocess.run(
        ["nix", "store", "info"], check=True, stdout=subprocess.DEVNULL, timeout=30
    )
    docker_root = output("docker", "info", "--format", "{{.DockerRootDir}}")
    if docker_root != "/var/lib/docker":
        raise RuntimeError(f"Docker uses {docker_root}, not /var/lib/docker")

    for backup in backups:
        shutil.rmtree(backup)
        print(f"Removed {backup}")


if __name__ == "__main__":
    if os.geteuid() != 0:
        raise PermissionError("Run as root")
    clean()
