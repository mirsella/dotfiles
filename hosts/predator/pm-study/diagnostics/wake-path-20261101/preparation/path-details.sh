set -euo pipefail
test "$(TZ=Europe/Paris date +%F)" = 2026-11-01
h=$(TZ=Europe/Paris date +%H)
test "$h" -ge 8 && test "$h" -lt 23
test "$(cat /proc/sys/kernel/random/boot_id)" = f9b45aa6-b820-4ab5-b18c-9324d8d53087
date --iso-8601=ns
for x in bios_date bios_vendor bios_version product_name; do printf '%s=' "$x"; cat "/sys/class/dmi/id/$x"; done
for dev in 0000:03:00.0 0000:03:00.1 0000:00:1c.3; do
  p=/sys/bus/pci/devices/$dev
  printf '\nDEVICE %s\n' "$dev"
  for x in d3cold_allowed power_state power/wakeup power/control power/runtime_status; do
    if test -e "$p/$x"; then printf '%s=' "$x"; cat "$p/$x"; else printf '%s=ABSENT\n' "$x"; fi
  done
  if test -e "$p/firmware_node"; then
    f=$(readlink -e "$p/firmware_node")
    printf 'firmware_node=%s\n' "$f"
    for x in path power_state real_power_state power/wakeup; do
      if test -e "$f/$x"; then printf '%s=' "$x"; cat "$f/$x"; else printf '%s=ABSENT\n' "$x"; fi
    done
  fi
done
printf '\nPCIe services\n'
find /sys/bus/pci_express/devices -maxdepth 1 -type l -printf '%f -> %l\n'
printf '\nDynamic debug selected original lines\n'
grep -E 'drivers/pci/pci-driver.c:.*pci_pm_suspend_noirq|drivers/pci/pci.c:.*pci_pme_active|drivers/acpi/device_pm.c:.*acpi_pm_set_device_wakeup|drivers/acpi/device_pm.c:.*acpi_enable_wakeup_device_power' /sys/kernel/debug/dynamic_debug/control
printf '\nBoot PCI firmware negotiation\n'
journalctl -k -b --no-pager -o cat | grep -E '_OSC|PME|00:1c.3|03:00.[01]|D3cold|D3hot|r8169|rtl8411' | head -n 100
printf '\nEEE\n'
ethtool --show-eee enp3s0f1
date --iso-8601=ns
