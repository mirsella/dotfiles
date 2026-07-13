# Main Workstation System Backup

Snapshot date: 2026-07-13

This is a machine-specific reference for host `main`. The entire `system/`
source directory is ignored by `chezmoi apply`; restoring these files is
intentionally a manual root operation.

## Hardware and firmware

- CPU: AMD Ryzen 9 5950X (16 cores, 32 threads, 90 C Tjmax)
- Motherboard: ASUS ROG STRIX X570-F GAMING
- BIOS: ASUS 5044, dated 2026-01-04
- Cooler: large air cooler
- Memory: 32 GiB

BIOS PBO profile, under `Advanced > AMD Overclocking > Precision Boost
Overdrive`:

- Precision Boost Overdrive: Advanced
- Limits: Manual
- PPT: 180 W
- TDC: 120 A
- EDC: 160 A
- Curve Optimizer: all cores, negative 10
- Scalar: 1X
- Max CPU boost clock override: 0 MHz
- Platform thermal limit: 90 C or Auto
- Ai Tweaker duplicate PBO: Auto
- CPU voltage, LLC, BCLK, and multiplier: Auto
- Fmax Enhancer: Disabled, if present

This profile passed about 23 minutes of staged `stress-ng` verification:
60 seconds all-method CPU, 5 minutes matrix product, every physical core pinned
for 45 seconds with FFT cycling, and 5 minutes all-method cycling. There were
no computation failures, MCE/EDAC/WHEA reports, reboots, or lockups. This is a
useful baseline, not a permanent stability guarantee. If unexplained CPU
stability errors appear, change Curve Optimizer from negative 10 to negative 5.

BIOS recovery reference:

- Official support page: <https://www.asus.com/supportonly/rog%20strix%20x570-f%20gaming/helpdesk_bios/>
- 5044 ZIP SHA-256: `90881a86d20d823dcc22b4e086aedaec4f9499a09b18396693ca8bac8ac174cb`
- 5044 CAP SHA-256: `9f2de9c5c2596392d0fde7a7f8b08f42732c509dac8d8a8ca069477e6a001f5d`
- 5020 ZIP SHA-256: `92398c87509e2c31c2303671584f041c0d67a4d152aae6e8c3351a02f01d6f`
- 5020 CAP SHA-256: `2b2632b22507bb222f4b1b17ccb6f34f382af36d7f692fb1c86b68cefeb0bad6`
- USB BIOS FlashBack filename for this board: `SX570FG.CAP`

The 5044 ZIP hash matched ASUS's published value. The 5020 archive was
downloaded from ASUS's official CDN, but ASUS no longer published its hash.

## Cooling

CoolerControl 4.3.1 drives all six Nuvoton nct6798 fan channels from CPU Tdie.
Four channels currently report connected fans. The profile is applied at boot.

Fan curve, temperature in C to duty in percent:

```text
0:30, 40:30, 55:40, 65:55, 75:75, 85:100, 100:100
```

The exact daemon config is backed up at `etc/coolercontrol/config.toml`.
`/etc/coolercontrol/.passwd`, certificates, and API credentials are excluded.
Stop `coolercontrold` before manually restoring the config, validate with
`coolercontrold --config`, then restart the daemon.

## Memory and swap

- `/dev/zram0`: `ram / 2` (about 15.6 GiB), zstd, priority 100
- `/swap/swapfile`: 32 GiB on a dedicated nested Btrfs subvolume, priority -1
- `vm.swappiness`: 100
- zswap: disabled with `zswap.enabled=0` in every Linux boot entry
- `vm.page-cluster`: effective value 0 during testing; it was not changed

The previous `vm.swappiness=1` plus enabled zswap caused severe reclaim stalls,
zram write failures, and desktop mini-freezes during Rust compilation. Runtime
testing with swappiness 100 and zswap disabled fixed the observed freezes.

The swapfile has NOCOW (`C` attribute). Its current resume offset is
`272657282`, but this value is valid only for this exact file. If the swapfile
is recreated, calculate a new offset with:

```bash
sudo btrfs inspect-internal map-swapfile -r /swap/swapfile
```

## Boot

- Boot loader: systemd-boot 261.1-1-arch
- Default entry: `linux-cachyos.conf`
- EFI System Partition: `/dev/nvme0n1p1`, UUID `3613-BAFF`
- Secure Boot: enabled with Microsoft vendor keys
- Root filesystem is not encrypted
- Resume uses the Btrfs filesystem UUID and swapfile offset above

Exact systemd-boot configuration is backed up under `boot/loader/`. The
mkinitcpio configuration is at `etc/mkinitcpio.conf`; its important settings
are `MODULES=(btrfs)`, `BINARIES=(/usr/bin/btrfs)`, and a `resume` hook.

Before restoring on changed storage, update every filesystem UUID, PARTUUID,
and resume offset in `fstab` and the boot entries. Secure Boot private keys are
not included in this repository.
