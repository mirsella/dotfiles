set -euo pipefail
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = f8eb6e44-85ae-4e56-bc00-587876699945
test "$(cat /sys/power/suspend_stats/success)" = 0
test "$(cat /sys/power/suspend_stats/fail)" = 0
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend --plain)"
old=predator-pm-diagnostic-d-freezer-01-20261030.service
test "$(systemctl show "$old" -p ActiveState --value)" = failed
test "$(systemctl show "$old" -p MainPID --value)" = 0
systemctl reset-failed "$old"
# The explained preflight refusal is archived; clear only its failed-unit state.
systemctl --failed --no-pager
umask 077
set -o noclobber
cat > /var/lib/predator-pm-diagnostic/d-freezer-02-20261030-plan.conf <<'PLAN'
schema=pm-test-1
id=d-freezer-02-20261030
boot=f8eb6e44-85ae-4e56-bc00-587876699945
generation=/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363
kernel=/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage
modules=/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules
runtime=/nix/store/krqqddfkhqipb8dzw02z56b24v9nnljp-predator-pm-study-0.1.0/bin/predator-pm-diagnostic
btime=1793381192
due=1793384612
window_start=1793379600
window_end=1793397600
level=freezer
ordinal=1
previous_id=none
previous_sha=none
pm_async=0
pm_trace=0
recorder=off
mem_sleep=deep
test_delay=5
tpm=initialized
ec=not-run
logged_in_users=allow-inhibitors-enforced
PLAN
/nix/store/krqqddfkhqipb8dzw02z56b24v9nnljp-predator-pm-study-0.1.0/bin/predator-pm-diagnostic check /var/lib/predator-pm-diagnostic/d-freezer-02-20261030-plan.conf 91e1616b734ee77a0077bccd130cecfe5621dfa0fba8b2326207e1064a8f4484
systemctl list-timers --all --no-pager 'predator-pm-*' night-suspend.timer nextcloud-cron.timer
systemctl show nextcloud-cron.timer -p LastTriggerUSec -p NextElapseUSecMonotonic -p AccuracyUSec -p RandomizedDelayUSec
journalctl -b f8eb6e4485ae4e56bc00587876699945 -u nextcloud-cron.service --since '2026-10-30 19:00:00' --no-pager -o short-iso-precise
