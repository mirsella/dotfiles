set -euo pipefail
date --iso-8601=seconds
test "$(cat /proc/sys/kernel/random/boot_id)" = f8eb6e44-85ae-4e56-bc00-587876699945
mkdir -p /home/mirsella/dev/dotfiles/.scratch/pm-phase-diagnostics-20261030
nix-store --realise /nix/store/krqqddfkhqipb8dzw02z56b24v9nnljp-predator-pm-study-0.1.0 --add-root /home/mirsella/dev/dotfiles/.scratch/pm-phase-diagnostics-20261030/runtime-root
install -d -m 0700 /var/lib/predator-pm-diagnostic
umask 077
set -o noclobber
cat > /var/lib/predator-pm-diagnostic/d-freezer-01-20261030-plan.conf <<'PLAN'
schema=pm-test-1
id=d-freezer-01-20261030
boot=f8eb6e44-85ae-4e56-bc00-587876699945
generation=/nix/store/zbq64a1n4yzr3knpsgzhiw29lvgs3b34-nixos-system-predator-26.11.20261023.3ec6363
kernel=/nix/store/82haph8j8hmbny68rzkis8dqrz9gqrr5-linux-6.18.58/bzImage
modules=/nix/store/1qbmxim06wak3qplmj747hb5gkyzgj9z-linux-6.18.58-modules
runtime=/nix/store/krqqddfkhqipb8dzw02z56b24v9nnljp-predator-pm-study-0.1.0/bin/predator-pm-diagnostic
btime=1793381192
due=1793383412
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
/nix/store/krqqddfkhqipb8dzw02z56b24v9nnljp-predator-pm-study-0.1.0/bin/predator-pm-diagnostic check /var/lib/predator-pm-diagnostic/d-freezer-01-20261030-plan.conf 9f2e4cbe873f8f5c5a8ead91f707deafcda8d7d260228cec54c7a43e6590c874
