set -euo pipefail
coproc WATCH { exec stdbuf -oL journalctl -f -n 50 --no-pager -o cat -u predator-pm-diagnostic-d-devices-02-20261030-review.service; }
pid=$WATCH_PID
printf 'follower_pid=%s\n' "$pid"
trap 'kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; printf "follower_reaped=%s\n" "$pid"' EXIT
trap 'exit 124' TERM INT
found=0
while IFS= read -r line <&"${WATCH[0]}"; do printf '%s\n' "$line"; if [[ "$line" == 'DIAG REVIEW READY d-devices-02-20261030' ]]; then found=1; break; fi; done
test "$found" = 1
date --iso-8601=seconds
