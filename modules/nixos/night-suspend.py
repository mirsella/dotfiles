"""One midnight attempt to suspend predator if nobody used it recently."""

import json
from datetime import datetime, timedelta
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
    # Caddy's NixOS module writes one JSON access record per line. A rotated
    # compressed log means requests were still arriving when it rolled over.
    if path.suffix == ".gz":
        return path.stat().st_mtime
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

    ports = "( sport = :22 or sport = :80 or sport = :443 or sport = :4096 or sport = :4097 or sport = :14096 or sport = :14097 or sport = :2283 )"
    if output("ss", "-Htn", "state", "established", ports).strip():
        return "active SSH or web connection"

    if not LOGS.is_dir():
        raise RuntimeError(f"missing Caddy access log directory: {LOGS}")
    cutoff = now - 30 * 60
    for path in LOGS.glob("access-*.log*"):
        if path.stat().st_mtime >= cutoff and last_request(path) >= cutoff:
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
    reason = blocker(time.time())
    if reason:
        print(f"night-suspend: blocked: {reason}")
        return

    now = datetime.now()
    wake = now.replace(hour=7, minute=0, second=0, microsecond=0)
    if wake <= now:
        wake += timedelta(days=1)
    alarm = int(wake.timestamp())
    subprocess.run(("rtcwake", "--mode=no", "--time", str(alarm)), check=True)
    print("night-suspend: RTC alarm set for 07:00, requesting suspend", flush=True)
    subprocess.run(("systemctl", "--check-inhibitors=yes", "suspend"), check=True)


if __name__ == "__main__":
    main()
