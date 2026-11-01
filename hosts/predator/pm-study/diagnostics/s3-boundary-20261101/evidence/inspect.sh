set -euo pipefail
export TZ=Europe/Paris
test "$(date +%F)" = 2026-11-01
h=$(date +%H); test "$((10#$h))" -ge 8; test "$((10#$h))" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
date --iso-8601=ns
cat /proc/sys/kernel/random/boot_id
grep '^btime ' /proc/stat
test "$(systemctl show systemd-suspend.service -p ActiveState --value)" = inactive
test "$(systemctl show systemd-suspend.service -p MainPID --value)" = 0
test -z "$(systemctl list-jobs --no-legend)"
test "$(cat /sys/power/suspend_stats/success)" = 2
test "$(cat /sys/power/suspend_stats/fail)" = 0
readlink -f /run/current-system /run/booted-system /run/booted-system/kernel /run/booted-system/kernel-modules
cat /proc/cmdline
systemctl is-system-running
systemctl --failed --no-legend
zpool status
systemctl show zfs-scrub.service -p ActiveState -p SubState -p Result
systemctl list-timers --all --no-pager
for f in /sys/power/mem_sleep /sys/power/pm_async /sys/power/pm_test /sys/power/pm_trace /sys/module/pm_stage/parameters/armed /sys/module/pm_stage/parameters/markers /sys/class/rtc/rtc0/wakealarm; do printf '%s=' "$f"; cat "$f"; done
printf '\nCAPTURE CONFIG\n'
if test -r /proc/config.gz; then zcat /proc/config.gz | grep -E 'CONFIG_(PSTORE|EFI_VARS_PSTORE|ACPI_APEI|SERIAL_8250|KEXEC|KPROBES|FTRACE|FUNCTION_TRACER|MAGIC_SYSRQ|IKCONFIG|DEVTMPFS)'; else printf '/proc/config.gz unavailable\n'; fi
printf '\nPSTORE STATE\n'
findmnt /sys/fs/pstore
for d in /sys/fs/pstore /var/lib/systemd/pstore; do
  if test -d "$d"; then find "$d" -maxdepth 2 -type f -printf '%p %s bytes\n'; else printf 'absent directory: %s\n' "$d"; fi
done
for p in /sys/module/pstore/parameters/* /sys/module/efi_pstore/parameters/* /sys/module/ramoops/parameters/*; do if test -f "$p"; then printf '%s=' "$p"; cat "$p"; fi; done
for m in pstore ramoops efi_pstore pstore_blk erst; do modinfo "$m" 2>&1 || true; done
printf '\nSERIAL AND TABLES\n'
cat /proc/tty/driver/serial
ls -l /dev/ttyS* /dev/ttyUSB* /dev/ttyACM* /dev/serial/by-id/* 2>/dev/null || true
find /sys/firmware/acpi/tables -maxdepth 2 -type f -printf '%f %s\n'
journalctl -k -b --no-pager -o short-monotonic | grep -Ei 'pstore|ramoops|ERST|APEI|serial|ttyS|console|reserved|efi:|Secure boot|Lockdown'
printf '\nMEMORY RESERVATIONS\n'
cat /proc/iomem
printf '\nFROZEN ROOTS\n'
find /nix/var/nix/gcroots -maxdepth 2 -type l -printf '%p -> %l\n'
printf '\nREMOTE REVIEW PREIMAGES\n'
cd /home/mirsella/dev/dotfiles
git status --short
git diff -- flake.lock
sha256sum flake.lock hosts/predator/pm-study/README.md hosts/predator/pm-study/diagnostics/README.md pkgs/predator-pm-study/wake_path.rs pkgs/predator-pm-study/wake_validation.rs pkgs/predator-pm-study/pm_stage.c
date --iso-8601=ns
