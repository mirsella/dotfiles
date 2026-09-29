#!/usr/bin/env python3
"""Check Btrfs headroom and optionally show a desktop warning."""

import argparse
import re
import subprocess


GIB = 2**30
RECLAIM_BELOW = 8 * GIB
RECLAIM_TARGET = 12 * GIB


def usage(path: str) -> tuple[float, int]:
    usage = subprocess.check_output(
        ["btrfs", "filesystem", "usage", "-b", path], text=True
    )
    unallocated = re.search(r"^\s*Device unallocated:\s*(\d+)\s*$", usage, re.M)
    metadata = re.findall(r"^Metadata,\w+: Size:(\d+), Used:(\d+)", usage, re.M)
    if not unallocated or not metadata:
        raise ValueError("Unexpected btrfs filesystem usage output")
    remaining = int(unallocated.group(1))
    allocated = sum(int(size) for size, _ in metadata)
    used = sum(int(occupied) for _, occupied in metadata)
    return 100 * used / allocated, remaining


def check(path: str, notify: bool = False) -> None:
    percent, remaining = usage(path)
    print(
        f"Btrfs {path}: metadata {percent:.1f}% used, {remaining / GIB:.2f} GiB unallocated"
    )
    if percent >= 90 or remaining < RECLAIM_BELOW:
        message = (
            f"Btrfs {path}: metadata {percent:.1f}% used; "
            f"{remaining / GIB:.2f} GiB unallocated. "
            "Inspect btrfs filesystem usage and the limited-balance service."
        )
        if notify:
            try:
                subprocess.run(
                    [
                        "notify-send",
                        "--urgency=critical",
                        "Btrfs metadata headroom low",
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
    _, remaining = usage(path)
    if remaining >= RECLAIM_BELOW:
        print(f"Btrfs {path}: {remaining / GIB:.2f} GiB unallocated; no balance needed")
        return
    for _ in range(4):
        subprocess.run(
            ["btrfs", "balance", "start", "-dusage=75,limit=5", path], check=True
        )
        _, available = usage(path)
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
