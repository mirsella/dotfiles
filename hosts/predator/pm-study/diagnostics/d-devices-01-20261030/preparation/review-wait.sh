set -euo pipefail
coproc WATCH { exec journalctl -f -n 50 --no-pager -o cat -u predator-pm-diagnostic-d-devices-01-20261030-review.service; }
pid=$WATCH_PID
trap 'kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true' EXIT
trap 'exit 124' TERM INT
found=0
while IFS= read -r line <&"${WATCH[0]}"; do printf '%s\n' "$line"; if [[ "$line" == 'DIAG REVIEW READY d-devices-01-20261030' ]]; then found=1; break; fi; done
test "$found" = 1
date --iso-8601=seconds
