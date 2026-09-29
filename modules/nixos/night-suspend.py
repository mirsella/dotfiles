"""Suspend predator during the night once users and web traffic are gone."""

import json
import re
from datetime import datetime
from pathlib import Path
import subprocess
import time


LOGS = Path("/var/log/caddy")
POOLS = ("fast", "tank")
MAINTENANCE = (
    "db-backup.service",
    "sanoid.service",
    "zfs-scrub.service",
    "zpool-trim.service",
    "nixos-upgrade.service",
    "nextcloud-setup.service",
    "nextcloud-cron.service",
)
# Keep in sync with the storage alert patterns in hosts/predator/storage-alerts.py.
STORAGE_EVENT = re.compile(
    r"\busb 2-(?:2|4)(?:\.\d+)*: (?:USB disconnect\b|reset .* USB device\b|"
    r"device descriptor read/.*error|device not accepting address|"
    r"unable to enumerate USB device)|"
    r"\buas_(?:eh_\w+|zap_pending)\b|"
    r"\bsd \S+: \[sd[a-z]+\] Synchronize Cache.*failed|"
    r"\bI/O error.*\bdev (?:sd[a-z]+\d*|dm-\d+)\b|"
    r"\bxhci_hcd\b.*(?:HC died|host controller not responding)"
)
STATUS_ROW = re.compile(
    r"^\s+(\S+)\s+(ONLINE|DEGRADED|FAULTED|UNAVAIL|OFFLINE|REMOVED|SUSPENDED)"
    r"\s+(\d+)\s+(\d+)\s+(\d+)\s*$"
)
STORAGE_QUIET = 30 * 60


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


def recent_storage_fault(now):
    lines = output(
        "journalctl", "-k", "--since", f"@{int(now - STORAGE_QUIET)}", "--no-pager"
    )
    return any(STORAGE_EVENT.search(line) for line in lines.splitlines())


def pool_repair(now, name):
    status = output("zpool", "status", name)
    if " in progress" in status:
        return f"ZFS {name} scan in progress"
    rows = []
    for line in status.splitlines():
        match = STATUS_ROW.match(line)
        if match:
            rows.append(match.groups())
    if not rows:
        return f"cannot parse zpool status for {name}"
    offline = sorted({row[1] for row in rows} - {"ONLINE"})
    if offline:
        return f"ZFS {name} vdevs not online: {', '.join(offline)}"
    if any(int(row[2]) or int(row[3]) for row in rows):
        return f"ZFS {name} has read or write errors"
    if recent_storage_fault(now):
        return f"recent storage faults, not clearing {name}"
    output("zpool", "clear", name)
    if output("zpool", "status", "-x", name).strip() != f"pool '{name}' is healthy":
        return f"ZFS {name} still unhealthy after zpool clear"
    print(f"night-suspend: cleared corrected errors on {name}", flush=True)
    return None


def pool_problem(now):
    imported = output("zpool", "list", "-H", "-o", "name").split()
    missing = [name for name in POOLS if name not in imported]
    if missing:
        return f"missing ZFS pools: {', '.join(missing)}"
    for name in POOLS:
        health = output("zpool", "status", "-x", name).strip()
        if health == f"pool '{name}' is healthy":
            continue
        problem = pool_repair(now, name)
        if problem:
            return problem
    return None


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
    problem = pool_problem(now)
    if problem:
        return problem
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
