set -euo pipefail
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = f8eb6e44-85ae-4e56-bc00-587876699945
test -z "$(systemctl list-timers --all --no-legend 'predator-pm-*' | awk '$1 != "-" {print}')"
systemctl list-timers --all --no-pager nextcloud-cron.timer
umask 077
set -o noclobber
cat > /var/lib/predator-pm-diagnostic/d-platform-02-20261030-plan.conf <<'PLAN'
schema=pm-test-2
id=d-platform-02-20261030
boot=f8eb6e44-85ae-4e56-bc00-587876699945
generation=/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363
kernel=/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage
modules=/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules
runtime=/nix/store/gqgwph5fdwycdl019dl2gwg6x32mvhan-predator-pm-study-0.1.0/bin/predator-pm-diagnostic
btime=1793381192
due=1793391812
window_start=1793379600
window_end=1793397600
level=platform
ordinal=5
previous_id=d-platform-01-20261030
previous_sha=0c5f6a3ef042e00ba0b5464ba249f86625389120f8a77004fb8a65763cb0697a
previous_kind=completed
pm_async=0
pm_trace=0
recorder=off
mem_sleep=deep
test_delay=5
tpm=initialized
ec=not-run
logged_in_users=allow-inhibitors-enforced
PLAN
/nix/store/gqgwph5fdwycdl019dl2gwg6x32mvhan-predator-pm-study-0.1.0/bin/predator-pm-diagnostic check /var/lib/predator-pm-diagnostic/d-platform-02-20261030-plan.conf 6ccb9f0f9f181058518acca38f230230513686fbf978ac2c18aa911d4f57c5f6
printf '<6>DIAG CAPTURE id=d-platform-02-20261030 boot=f8eb6e44-85ae-4e56-bc00-587876699945 due=1793391812\n' > /dev/kmsg
journalctl --sync
