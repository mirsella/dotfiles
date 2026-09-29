#!/usr/bin/env python3
"""Check Btrfs headroom and optionally show a desktop warning."""

import argparse
import re
import shutil
import subprocess


GIB = 2**30
RECLAIM_BELOW = 8 * GIB
RECLAIM_TARGET = 12 * GIB


def device_unallocated(path: str) -> int:
    output = subprocess.check_output(
        ["btrfs", "filesystem", "usage", "-b", path], text=True
    )
    unallocated = re.search(r"^\s*Device unallocated:\s*(\d+)\s*$", output, re.M)
    if not unallocated:
        raise ValueError("Unexpected btrfs filesystem usage output")
    return int(unallocated.group(1))


def check(path: str, notify: bool = False) -> None:
    remaining = device_unallocated(path)
    disk = shutil.disk_usage(path)
    percent = 100 * (disk.total - disk.free) / disk.total
    print(
        f"Btrfs {path}: filesystem {percent:.1f}% full, {remaining / GIB:.2f} GiB device-unallocated"
    )
    if percent >= 90 or remaining < RECLAIM_BELOW:
        message = (
            f"Btrfs {path}: filesystem {percent:.1f}% full; "
            f"{remaining / GIB:.2f} GiB device-unallocated. "
            "Inspect filesystem usage and the limited-balance service."
        )
        if notify:
            try:
                subprocess.run(
                    [
                        "notify-send",
                        "--urgency=critical",
                        "Filesystem space pressure",
                        message,
                    ],
                    check=True,
                )
            except (OSError, subprocess.CalledProcessError) as error:
                raise RuntimeError(
                    f"{message} Desktop notification failed: {error}"
                ) from error
        raise RuntimeError(message)


def reclaim(path: str) -> None:
    remaining = device_unallocated(path)
    if remaining >= RECLAIM_BELOW:
        print(f"Btrfs {path}: {remaining / GIB:.2f} GiB unallocated; no balance needed")
        return
    for _ in range(4):
        subprocess.run(
            ["btrfs", "balance", "start", "-dusage=75,limit=5", path], check=True
        )
        available = device_unallocated(path)
        print(
            f"Btrfs {path}: {available / GIB:.2f} GiB unallocated after limited balance"
        )
        if available >= RECLAIM_TARGET:
            return
        if available <= remaining:
            raise RuntimeError(
                "Limited balance did not free device space; inspect Btrfs usage"
            )
        remaining = available
    raise RuntimeError(
        f"Btrfs {path} still has only {remaining / GIB:.2f} GiB unallocated after four limited passes"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--notify", action="store_true", help="send a desktop notification on alert"
    )
    mode.add_argument(
        "--reclaim",
        action="store_true",
        help="reclaim sparse data chunks before metadata needs space",
    )
    parser.add_argument("path", nargs="?", default="/")
    args = parser.parse_args()
    if args.reclaim:
        reclaim(args.path)
    else:
        check(args.path, args.notify)
