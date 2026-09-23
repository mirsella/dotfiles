"""Suspend predator during the night once users and web traffic are gone."""

import json
from datetime import datetime
from pathlib import Path
import subprocess
import time


LOGS = Path("/var/log/caddy")
MAINTENANCE = (
    "db-backup.service",
    "sanoid.service",
    "zfs-scrub.service",
    "zpool-trim.service",
    "nixos-upgrade.service",
    "nextcloud-setup.service",
    "nextcloud-cron.service",
)


def output(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def last_request(path):
    with path.open("rb") as log:
        size = log.seek(0, 2)
        log.seek(max(0, size - 131072))
        lines = log.read().splitlines()
    if not lines:
        return 0
    return float(json.loads(lines[-1])["ts"])


def blocker(now):
    sessions = json.loads(output("loginctl", "list-sessions", "--json=short"))
    if any(session["class"].startswith("user") for session in sessions):
        return "SSH or local login session"

    ports = (
        "( sport = :22 or sport = :80 or sport = :443 or sport = :4096"
        " or sport = :4097 or sport = :14096 or sport = :14097 or sport = :2283 )"
        " and not dst 127.0.0.0/8 and not dst ::1/128"
    )
    if output("ss", "-Htn", "state", "established", ports).strip():
        return "active SSH or web connection"

    if not LOGS.is_dir():
        raise RuntimeError(f"missing Caddy access log directory: {LOGS}")
    cutoff = now - 30 * 60
    for path in LOGS.glob("access-*.log*"):
        if path.stat().st_mtime < cutoff:
            continue
        # A fresh compressed rotation may contain recent requests; fail closed.
        if path.suffix == ".gz" or last_request(path) >= cutoff:
            return "web request within 30 minutes"

    busy = output(
        "systemctl",
        "list-units",
        "--no-pager",
        "--no-legend",
        "--plain",
        "--state=active,activating,reloading,deactivating",
        *MAINTENANCE,
    )
    if busy.strip():
        return f"maintenance: {busy.strip()}"
    workers = subprocess.run(("pgrep", "-G", "nixbld"), capture_output=True)
    if workers.returncode == 0:
        return "Nix build workers"
    if workers.returncode != 1:
        raise RuntimeError(f"pgrep failed: {workers.stderr.decode()}")
    return None


def main():
    now = datetime.now()
    # A calendar timer can run late after wake; never suspend in the daytime.
    if now.hour >= 7:
        print("night-suspend: outside the overnight window")
        return

    reason = blocker(time.time())
    if reason:
        print(f"night-suspend: blocked: {reason}")
        return

    alarm = int(now.replace(hour=7, minute=0, second=0, microsecond=0).timestamp())
    subprocess.run(("rtcwake", "--mode=no", "--time", str(alarm)), check=True)
    print("night-suspend: RTC alarm set for 07:00, requesting suspend", flush=True)
    subprocess.run(("systemctl", "--check-inhibitors=yes", "suspend"), check=True)


if __name__ == "__main__":
    main()
