#!/usr/bin/env python3
"""Recursive inotify dirty-flag daemon.

Watches archive dataset trees and touches a flag file in FLAGDIR whenever
anything underneath is created, deleted, modified or moved. The daily
snapshot timers check the flag instead of touching the (possibly sleeping)
disks: no flag file means provably no writes, so snapshots are skipped
without waking anything.

Safety properties (all conservative):
- on startup every flag is marked dirty (writes may have happened while down),
- on queue overflow every flag is marked dirty,
- on any unexpected error every flag is marked dirty and the process exits
  non-zero so systemd restarts it (which marks dirty again).
"""

import ctypes
import os
import struct
import sys

libc = ctypes.CDLL("libc.so.6", use_errno=True)

IN_CREATE = 0x100
IN_DELETE = 0x200
IN_MODIFY = 0x2
IN_MOVED_FROM = 0x40
IN_MOVED_TO = 0x80
IN_IGNORED = 0x8000
IN_Q_OVERFLOW = 0x4000
IN_ISDIR = 0x40000000
MASK = IN_CREATE | IN_DELETE | IN_MODIFY | IN_MOVED_FROM | IN_MOVED_TO

ROOTS = {
    "/srv/storage": "tank",
    "/srv/backup": "backup",
    "/var/lib/nextcloud/data": "fast",
}


def log(msg):
    print(msg, flush=True)


def touch(flagdir, flag):
    path = os.path.join(flagdir, flag + ".dirty")
    with open(path, "a"):
        pass


def main(flagdir):
    os.makedirs(flagdir, exist_ok=True)
    fd = libc.inotify_init1(os.O_CLOEXEC)
    if fd < 0:
        raise OSError(ctypes.get_errno(), "inotify_init1 failed")

    watched = {}

    def add_watch(path, flag):
        wd = libc.inotify_add_watch(fd, os.fsencode(path), MASK)
        if wd < 0:
            return
        watched[wd] = (path, flag)

    for root, flag in ROOTS.items():
        if not os.path.isdir(root):
            log(f"root missing, skipping: {root}")
            continue
        for dirpath, _dirnames, _files in os.walk(root):
            add_watch(dirpath, flag)
    log(f"watching {len(watched)} dirs")

    for flag in set(ROOTS.values()):
        touch(flagdir, flag)
    log("startup: all flags marked dirty")

    try:
        while True:
            data = os.read(fd, 256 * 1024)
            off = 0
            while off + 16 <= len(data):
                wd, mask, _cookie, length = struct.unpack_from("iIII", data, off)
                name = data[off + 16 : off + 16 + length].rstrip(b"\0")
                off += 16 + length
                if mask & IN_Q_OVERFLOW:
                    log("queue overflow, marking everything dirty")
                    for flag in set(ROOTS.values()):
                        touch(flagdir, flag)
                    continue
                if mask & IN_IGNORED:
                    watched.pop(wd, None)
                    continue
                entry = watched.get(wd)
                if entry is None:
                    continue
                _path, flag = entry
                if mask & (
                    IN_MODIFY | IN_CREATE | IN_DELETE | IN_MOVED_FROM | IN_MOVED_TO
                ):
                    touch(flagdir, flag)
                if mask & IN_CREATE and mask & IN_ISDIR:
                    full = os.path.join(_path, os.fsdecode(name))
                    if os.path.isdir(full):
                        add_watch(full, flag)
    except Exception as exc:
        log(f"error, marking everything dirty and exiting: {exc}")
        for flag in set(ROOTS.values()):
            try:
                touch(flagdir, flag)
            except OSError:
                pass
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
