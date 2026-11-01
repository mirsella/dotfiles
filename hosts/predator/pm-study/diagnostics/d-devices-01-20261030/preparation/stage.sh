set -euo pipefail
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = f8eb6e44-85ae-4e56-bc00-587876699945
test -z "$(systemctl list-timers --all --no-legend 'predator-pm-*' | awk '$1 != "-" {print}')"
systemctl list-timers --all --no-pager nextcloud-cron.timer
umask 077
set -o noclobber
cat > /var/lib/predator-pm-diagnostic/d-devices-01-20261030-plan.conf <<'PLAN'
schema=pm-test-2
id=d-devices-01-20261030
boot=f8eb6e44-85ae-4e56-bc00-587876699945
generation=/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363
kernel=/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage
modules=/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules
runtime=/nix/store/gqgwph5fdwycdl019dl2gwg6x32mvhan-predator-pm-study-0.1.0/bin/predator-pm-diagnostic
btime=1793381192
due=1793387912
window_start=1793379600
window_end=1793397600
level=devices
ordinal=2
previous_id=d-freezer-02-20261030
previous_sha=8ad223af8553390f307e19a84aa2b9f825fc9f0fb5d1a9d567d515696dd32e2c
previous_kind=revalidated
pm_async=0
pm_trace=0
recorder=off
mem_sleep=deep
test_delay=5
tpm=initialized
ec=not-run
logged_in_users=allow-inhibitors-enforced
PLAN
/nix/store/gqgwph5fdwycdl019dl2gwg6x32mvhan-predator-pm-study-0.1.0/bin/predator-pm-diagnostic check /var/lib/predator-pm-diagnostic/d-devices-01-20261030-plan.conf 7c26f9ca1fe474eb65113f94f43bf444c621ab1e446e39037d0a6ef5904beb2c
printf '<6>DIAG CAPTURE id=d-devices-01-20261030 boot=f8eb6e44-85ae-4e56-bc00-587876699945 due=1793387912\n' > /dev/kmsg
journalctl --sync
