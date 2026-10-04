"""Suspend predator during the night once users and web traffic are gone."""

import json
from datetime import datetime
from pathlib import Path
import subprocess
import time
from storage_events import STORAGE_EVENT


LOGS = Path("/var/log/caddy")
POOLS = ("fast", "tank")
MAINTENANCE = (
    "db-backup.service",
    "recovery-backup.service",
    "sanoid.service",
    "syncoid-*.service",
    "zfs-scrub.service",
    "zpool-trim.service",
    "nixos-upgrade.service",
    "nextcloud-setup.service",
    "nextcloud-cron.service",
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


def pool_repair(now, pool):
    name = pool["name"]
    if pool.get("scan_stats", {}).get("state") == "SCANNING":
        return f"ZFS {name} scan in progress"
    vdevs = pool["vdevs"].values()
    offline = sorted({pool["state"], *(vdev["state"] for vdev in vdevs)} - {"ONLINE"})
    if offline:
        return f"ZFS {name} vdevs not online: {', '.join(offline)}"
    if any(vdev["read_errors"] or vdev["write_errors"] for vdev in vdevs):
        return f"ZFS {name} has read or write errors"
    if pool["error_count"]:
        return f"ZFS {name} has permanent data errors"
    if not any(vdev["checksum_errors"] for vdev in vdevs):
        return f"ZFS {name} requires attention"
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
    unhealthy = json.loads(
        output("zpool", "status", "-xj", "--json-int", "--json-flat-vdevs")
    )["pools"]
    for name in POOLS:
        if name not in unhealthy:
            continue
        problem = pool_repair(now, unhealthy[name])
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


def warm_storage():
    disks = [
        disk["name"]
        for disk in json.loads(output("lsblk", "-Jdp", "-o", "NAME,TRAN"))[
            "blockdevices"
        ]
        if disk["tran"] == "usb"
    ]
    for disk in disks:
        subprocess.run(
            ("dd", f"if={disk}", "of=/dev/null", "bs=512", "count=1", "iflag=direct"),
            check=True,
            timeout=90,
        )
    if disks:
        print(f"night-suspend: warmed {' '.join(disks)}", flush=True)


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

    warm_storage()

    alarm = int(now.replace(hour=7, minute=0, second=0, microsecond=0).timestamp())
    subprocess.run(("rtcwake", "--mode=no", "--time", str(alarm)), check=True)
    print("night-suspend: RTC alarm set for 07:00, requesting suspend", flush=True)
    subprocess.run(("systemctl", "--check-inhibitors=yes", "suspend"), check=True)


if __name__ == "__main__":
    main()
