#!/usr/bin/env python3
"""Install root-owned Arch maintenance on Btrfs workstations."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


SOURCE = Path(__file__).resolve().parent
SYSTEMD = Path("/etc/systemd/system")


def run(*args: str) -> None:
    subprocess.run(args, check=True)


def write_if_changed(path: Path, text: str) -> bool:
    if path.exists() and path.read_text() == text:
        return False
    path.parent.mkdir(parents=True, exist_ok=True)
    previous = path.stat() if path.exists() else None
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as file:
        file.write(text)
        temporary = Path(file.name)
    try:
        os.chmod(temporary, previous.st_mode & 0o777 if previous else 0o644)
        if previous:
            os.chown(temporary, previous.st_uid, previous.st_gid)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)
    return True


def set_values(path: Path, values: dict[str, str], separator: str) -> bool:
    lines = []
    seen = set()
    for line in path.read_text().splitlines():
        assignment = re.match(r"^\s*([\w-]+)\s*=", line)
        key = assignment.group(1) if assignment else None
        if key not in values:
            lines.append(line)
        elif key not in seen:
            lines.append(f"{key}{separator}{values[key]}")
            seen.add(key)
    lines.extend(
        f"{key}{separator}{value}" for key, value in values.items() if key not in seen
    )
    return write_if_changed(path, "\n".join(lines) + "\n")


def main() -> None:
    if os.geteuid() != 0:
        raise PermissionError("Run this installer with sudo")
    if (
        subprocess.check_output(
            ["findmnt", "-n", "-o", "FSTYPE", "/"], text=True
        ).strip()
        != "btrfs"
    ):
        raise RuntimeError("Arch storage maintenance requires a Btrfs root")

    timers = ["btrfs-scrub@-.timer"]
    if not Path("/usr/lib/systemd/system/btrfs-scrub@.timer").exists():
        raise FileNotFoundError(
            "Install btrfs-progs for its native monthly scrub timer"
        )
    unit_names = ["btrfs-balance-limited", "btrfs-space-check"]
    nix_changed = False

    if shutil.which("nix-collect-garbage"):
        nix_changed = set_values(
            Path("/etc/nix/nix.conf"),
            {
                "min-free": str(10 * 2**30),
                "max-free": str(20 * 2**30),
                "auto-optimise-store": "true",
            },
            " = ",
        )
        unit_names.append("nix-gc")
    else:
        print("Nix not installed; skipping Nix GC")

    if shutil.which("timeshift"):
        path = Path("/etc/timeshift/timeshift.json")
        config = json.loads(path.read_text())
        schedule = {
            "schedule_monthly": "true",
            "count_monthly": "1",
            "schedule_weekly": "true",
            "count_weekly": "2",
            "schedule_daily": "false",
            "count_daily": "0",
            "schedule_hourly": "false",
            "count_hourly": "0",
            "schedule_boot": "false",
            "count_boot": "0",
        }
        if any(config.get(key) != value for key, value in schedule.items()):
            config.update(schedule)
            write_if_changed(path, json.dumps(config, indent=2) + "\n")
        autosnap = Path("/etc/timeshift-autosnap.conf")
        if autosnap.exists():
            set_values(autosnap, {"maxSnapshots": "2"}, "=")
    else:
        print("Timeshift not installed; skipping snapshot retention")

    if shutil.which("paccache"):
        set_values(Path("/etc/conf.d/pacman-contrib"), {"PACCACHE_ARGS": '"-k 2"'}, "=")
        timers.append("paccache.timer")
    else:
        print("paccache not installed; skipping pacman cache pruning")

    if shutil.which("docker"):
        unit_names.append("docker-builder-prune")
    else:
        print("Docker not installed; skipping build cache pruning")

    if shutil.which("fail2ban-client"):
        changed = write_if_changed(
            Path("/etc/fail2ban/jail.d/sshd.local"),
            (SOURCE / "fail2ban-jail.local").read_text(),
        )
        run("systemctl", "enable", "fail2ban.service")
        run("systemctl", "restart" if changed else "start", "fail2ban.service")
    else:
        print("fail2ban not installed; skipping SSH brute-force jail")

    units_changed = False
    for name in unit_names:
        for suffix in ("service", "timer"):
            unit = f"{name}.{suffix}"
            units_changed |= write_if_changed(
                SYSTEMD / unit, (SOURCE / unit).read_text()
            )
        timers.append(f"{name}.timer")
    units_changed |= write_if_changed(
        Path("/usr/local/libexec/btrfs-space-check.py"),
        (SOURCE / "btrfs-space-check.py").read_text(),
    )
    journal_changed = write_if_changed(
        Path("/etc/systemd/journald.conf.d/50-maintenance.conf"),
        (SOURCE / "journald.conf").read_text(),
    )
    if units_changed:
        run("systemctl", "daemon-reload")
    run("systemctl", "enable", "--now", *timers)
    if journal_changed:
        run("systemctl", "restart", "systemd-journald.service")
    if nix_changed:
        run("systemctl", "try-restart", "nix-daemon.service")
    print("Enabled maintenance timers:", ", ".join(timers))


if __name__ == "__main__":
    main()
