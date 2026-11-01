set -euo pipefail
test "$(date +%s)" -lt 1793394807
coproc WATCH { exec stdbuf -oL journalctl -f -n 0 --no-pager -o cat; }
pid=$WATCH_PID
printf 'follower_pid=%s\n' "$pid"
trap 'kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; printf "follower_reaped=%s\n" "$pid"' EXIT
trap 'exit 124' TERM INT
found=0
while IFS= read -r line <&"${WATCH[0]}"; do if [[ "$line" == 'DIAG REVIEW READY d-core-01-20261030' ]]; then printf '%s\n' "$line"; found=1; break; fi; done
test "$found" = 1
date --iso-8601=seconds
