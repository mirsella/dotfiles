# Predator reinstall and recovery

The `predator` target defines the running server. `predator-install` is a small
installed system with SSH, ZFS tools, signed boot, and root TPM/passphrase
unlock. Use it when application state needs restoring before the server
services start. It is not a live ISO. Neither target partitions or formats disks.

Both targets import `hosts/predator-boot.nix` for the signing hierarchy, root
unlock, EFI permissions, ZFS host ID, and USB storage quirk. The full server adds
data-disk unlocking and pool management through `modules/nixos/storage.nix`.

Run `nix eval --impure --json --file tests/recovery.nix` to check both targets,
the standalone formatter, and the workstation mount configuration without
building or touching disks.

Test the HDD unlock timeout and retry paths with command doubles:

```bash
nix eval --raw path:.#nixosConfigurations.predator.config.systemd.services.tank-unlock.script \
  --no-update-lock-file | python3 tests/tank-unlock.py
```

`disko.nix` is a separate formatter for the three data disks. Use it only for
the fresh-data-disk procedure below. A normal OS reinstall preserves their
LUKS headers, TPM tokens, recovery slots, filesystems, and ZFS pools.

## Disk map and state outside Git

These paths identify the existing disks, not replacement hardware:

| Mapper | Device under `/dev/disk/by-id/` | Contents | Normal unlock |
| --- | --- | --- | --- |
| `crypt-root` | `ata-HFS128G39TND-N210A_EI76N026711106D68-part2` | ext4 root | TPM |
| `fast-crypt` | `ata-CT240BX500SSD1_2004E3E6DE68-part1` | ZFS `fast` | TPM |
| `tank1-crypt` | `wwn-0x5000c500aa3cc143-part1` | ZFS `tank` mirror member | Keyfile on root |
| `tank2-crypt` | `wwn-0x500003961228993f-part1` | ZFS `tank` mirror member | Keyfile on root |

The Hynix SSD's first partition is the FAT EFI system partition, mounted at
`/boot`. Its current filesystem UUID is `16CC-BD3D`.

| State | Location to preserve or restore |
| --- | --- |
| Signing hierarchy, including private keys | `/var/lib/sbctl/` |
| HDD unlock keys | `/etc/luks/tank1.key`, `/etc/luks/tank2.key` |
| SOPS identity | `/etc/ssh/ssh_host_ed25519_key` and its `.pub` file |
| Recovery passphrases | Proton Pass, Personal vault, `predator luks recovery` |
| TPM sealed objects and policy | Each disk's LUKS2 header, tied to this TPM |
| Secure Boot trust and enabled state | Firmware variables |

There are no root or `fast` keyfiles to restore. Their old keyfile slots were
revoked. Do not put LUKS keys, passphrases, or signing private keys in Git or
Nix strings. The initrd reads the TPM token from the LUKS header; supplying a
`keyFile` alongside `tpm2-device=auto` makes systemd interpret that file as a
sealed TPM blob.

## Reinstall while keeping the existing disks

### 1. Boot a rescue system and mount root

Use a recent NixOS live USB in UEFI mode. If its bootloader is not trusted by
Secure Boot, temporarily disable Secure Boot while retaining the enrolled
keys. Leave the TPM enabled and do not clear it. The changed boot state can
prevent automatic TPM unlock; use the recovery passphrases instead.

Commands below run in a root Bash shell in the live system. Run them in order
and stop on any error. Confirm the disk serials with `lsblk` before opening them:

```bash
sudo -i
loadkeys fr
lsblk -o NAME,SIZE,MODEL,SERIAL,FSTYPE,UUID,MOUNTPOINTS
root_disk=/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68
cryptsetup open "${root_disk}-part2" crypt-root
mount /dev/mapper/crypt-root /mnt
mkdir -p /mnt/boot
mount "${root_disk}-part1" /mnt/boot
```

This procedure does not run Disko, `luksFormat`, `mkfs`, or `zpool create`.
The data disks can stay locked until the installed system boots.

### 2. Restore the checkout and required files

The local development checkout is `~/dev/dotfiles`. Predator's deployed
checkout is `/home/mirsella/dev/nixos`, which the upgrade service also uses.
Place the reviewed configuration, including `flake.lock`, at that target path:

```bash
repo=/mnt/home/mirsella/dev/nixos
test -f "$repo/flake.nix"
test -f "$repo/flake.lock"
```

Use the surviving checkout or copy it from another machine. A fresh Git clone
only includes committed changes.

On a surviving root filesystem, the signing keys, HDD keys, and SSH host key
should already be present. If restoring root from backup, restore them under
`/mnt` before installation. Preserve root ownership and private-key permissions:

```bash
test -s /mnt/var/lib/sbctl/keys/PK/PK.key
test -s /mnt/var/lib/sbctl/keys/KEK/KEK.key
test -s /mnt/var/lib/sbctl/keys/db/db.key
test -s /mnt/etc/luks/tank1.key
test -s /mnt/etc/luks/tank2.key
test -s /mnt/etc/ssh/ssh_host_ed25519_key
chmod 0400 /mnt/etc/luks/tank1.key /mnt/etc/luks/tank2.key
chmod 0600 /mnt/etc/ssh/ssh_host_ed25519_key
```

Restore the whole `sbctl` directory, including its certificates and metadata,
not just the three files checked above. Missing files require the replacement
procedures below; do not proceed with the full server target until resolved.

If the SOPS host identity changed, derive its age recipient with
`ssh-to-age -i <new-host-public-key>`, update the `predator-host` entry in
`.sops.yaml`, and run `sops updatekeys` on each encrypted file in `secrets/`
from a machine with an existing authorized identity. Updating `.sops.yaml`
alone does not re-encrypt existing secrets.

Preserve application state on root as well. Nextcloud's database/configuration
and Immich's PostgreSQL data are not recreated from the media files. If root
was lost, install `predator-install` first and restore application state before
switching to `predator`. See [Immich's restore notes](immich.md).
For Nextcloud sharing defaults, mount recreation, TOTP recovery and per-user
settings, see [Nextcloud preferences and recovery](nextcloud.md).

### 3. Install the selected target

Use `predator` when existing application state and secrets are intact. Set
`target=predator-install` for the minimal recovery system instead. Both targets
require the signing bundle at `/mnt/var/lib/sbctl` before installation.

Run the build detached so an SSH disconnect does not stop it:

```bash
target=predator
systemd-run --unit=predator-reinstall --property=Type=oneshot \
  --setenv=PATH="$PATH" --no-block \
  "$(command -v nixos-install)" --root /mnt \
  --flake "path:$repo#$target" --no-root-password \
  --max-jobs 1 --cores 2 --no-update-lock-file
journalctl -fu predator-reinstall
```

After the command finishes, check the result rather than just the last log line:

```bash
systemctl show predator-reinstall -p ActiveState -p Result -p ExecMainStatus
```

Require `ActiveState=inactive`, `Result=success`, and `ExecMainStatus=0` before
rebooting. The `path:` flake reference also avoids Git ownership errors when
root reads a user-owned checkout. The minimal target provides the configured
SSH authorized keys; the install command does not create a root login password.

If Secure Boot was disabled for rescue, re-enable it with the existing keys.
Boot the installed system and enter recovery passphrases if requested. Do TPM
re-enrollment from this installed, signed boot, not from the live USB.

## Missing HDD keyfiles

Restoring the original keyfiles is enough if their LUKS slots still exist.
Verify each restored file with `cryptsetup open --test-passphrase --key-file
FILE DEVICE` before relying on it.

If a keyfile is lost, a recovery passphrase can authorize a new one. On the
installed recovery system, run the following once per HDD. This example uses
`tank1`; use the corresponding `tank2` path and device for the other disk:

```bash
sudo -i
(
set -e
key=/etc/luks/tank1.key
device=/dev/disk/by-id/wwn-0x5000c500aa3cc143-part1
install -d -m 0700 /etc/luks
test ! -e "$key"
umask 077
dd if=/dev/urandom of="$key" bs=64 count=1 status=none
cryptsetup luksAddKey "$device" "$key"
chmod 0400 "$key"
cryptsetup open --test-passphrase --key-file "$key" "$device"
)
```

`luksAddKey` prompts for an existing recovery passphrase. Inspect `cryptsetup
luksDump` before removing a superseded keyfile slot. Keep the recovery and TPM
slots; slot numbers depend on enrollment history. Do not recreate root/`fast`
keyfiles or embed HDD keys in the initrd.

## Access existing pools from the minimal system

The minimal target includes ZFS support but does not unlock the data disks or
start application services. After restoring or enrolling the HDD keyfiles,
open the existing containers and import their pools:

```bash
sudo -i
cryptsetup open /dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68-part1 fast-crypt
cryptsetup open --key-file /etc/luks/tank1.key \
  /dev/disk/by-id/wwn-0x5000c500aa3cc143-part1 tank1-crypt
cryptsetup open --key-file /etc/luks/tank2.key \
  /dev/disk/by-id/wwn-0x500003961228993f-part1 tank2-crypt
zpool import -N -d /dev/mapper fast
zpool import -N -d /dev/mapper tank
zfs mount fast/data
zfs mount tank/archive
zfs mount tank/backup
mkdir -p /var/lib/nextcloud/data
mount -t zfs fast/ncdata /var/lib/nextcloud/data
```

Skip individual opens, imports, or mounts that are already active. If import
reports that another host used the pool, confirm it is offline and that the
host ID is correct before considering a forced import. Use the database backups
under `/srv/backup/db` and the media datasets to restore application state.
Export the pools cleanly before rebooting out of the minimal system:

```bash
umount /var/lib/nextcloud/data
zpool export fast
zpool export tank
```

## Missing signing keys or firmware trust

If the signing bundle survives and firmware still trusts its certificate, keep
both. Reinstalling NixOS alone does not require clearing or re-enrolling firmware
keys.

If the signing bundle is lost, create a replacement in the live environment
before installing either target. This creates a new identity that the firmware
must subsequently trust:

```bash
nix shell --inputs-from "$repo" nixpkgs#sbctl --command sbctl create-keys
install -d -m 0755 /mnt/var/lib
cp -a /var/lib/sbctl /mnt/var/lib/
```

Use that block only in a live system without an existing signing bundle, and
with an empty target `/mnt/var/lib/sbctl`. Boot the signed installation with
Secure Boot temporarily disabled, then enroll the new hierarchy. The same
enrollment procedure applies when the original bundle was restored but firmware
trust was reset.

1. Preserve the existing firmware certificates and dbx revocations before
   clearing keys. `sbctl export-enrolled-keys` omits dbx. On this Acer, the
   earlier export lives at `/var/lib/sbctl/pre-enrollment-20260921/enrolled/` if
   that directory was restored. A newer export takes precedence after dbx updates.
2. In Acer's Security tab, deleting just the Platform Key would enter Setup
   Mode while preserving the databases. The PH317-51 menu seen during setup
   only offered **Erase all Secure Boot Setting**, which also reduced dbx.
   That option requires restoring the saved revocations afterward. Keep
   **Clear TPM (TCM)** untouched.
3. Boot Linux with Secure Boot disabled and confirm `sbctl status` reports
   Setup Mode enabled. Restore dbx before installing the Platform Key. With
   `efitools` available, the operation is `efi-updatevar -a -e -f
   /path/to/saved/dbx.esl dbx`. For an immutable efivar, remove its immutable
   flag for the write and restore it afterward. Verify the saved revocation
   entries are present. An `.esl` backup excludes the first four EFI-variable
   attribute bytes; a raw efivarfs copy is not an ESL file.
4. Run `sudo sbctl enroll-keys --microsoft --firmware-builtin`. This installs
   our signing hierarchy with Microsoft/OEM trust for firmware and option ROMs.
   Check `sbctl verify` and confirm Setup Mode is disabled.
5. In BIOS Boot settings, enable Secure Boot, retain UEFI mode and the enrolled
   custom keys, then save and reboot. Use recovery passphrases if prompted.

Follow the [Lanzaboote enrollment guide](https://nix-community.github.io/lanzaboote/getting-started/enable-secure-boot.html)
for the firmware enrollment details. A standalone external kernel payload may
appear unsigned in `sbctl verify`; Lanzaboote's signed stub authenticates its
referenced payloads. Verify the signed bootloader and Lanzaboote entries.

## TPM re-enrollment after the final signed boot

First confirm `sbctl status` reports Secure Boot enabled and Setup Mode
disabled. This machine currently has an active SHA1 PCR bank; the tested policy
binds PCRs 0, 2, and 7. Check `tpm2_getcap -T device:/dev/tpmrm0 pcrs` if firmware
or hardware changed. A different TPM or PCR-bank configuration needs a reviewed
policy rather than blindly reusing this one.

The following prompts for an existing recovery passphrase on each volume.
It enrolls the replacement token before wiping the previous TPM token. It does
not wipe password/keyfile slots:

```bash
(
set -e
for device in \
  /dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68-part2 \
  /dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68-part1 \
  /dev/disk/by-id/wwn-0x5000c500aa3cc143-part1 \
  /dev/disk/by-id/wwn-0x500003961228993f-part1
do
  sudo systemd-cryptenroll --tpm2-device=auto \
    --tpm2-pcrs=0:sha1+2:sha1+7:sha1 --wipe-slot=tpm2 "$device"
  sudo env LD_LIBRARY_PATH=/run/current-system/systemd/lib/cryptsetup \
    cryptsetup open --type luks2 --test-passphrase \
    --token-only --token-type systemd-tpm2 "$device"
done
)
```

Require successful enrollment and verification for every volume. The explicit
library path is needed by the standalone `cryptsetup` CLI on this NixOS setup;
the initrd's `systemd-cryptsetup` already finds the token plugin. Root and `fast`
use their tokens during boot. The HDDs normally use the keyfiles on encrypted
root even though their TPM tokens are also enrolled.

When the minimal system's restoration work is complete, install the full
configuration for the next boot. Ensure `mirsella` owns the deployed checkout;
the upgrade service updates its lock file as that user. Run this as `mirsella`:

```bash
sudo systemd-run --unit=predator-restored-boot --uid=mirsella \
  --property=Type=oneshot --setenv=PATH=/run/wrappers/bin:/run/current-system/sw/bin \
  --no-block /run/current-system/sw/bin/nixos-rebuild boot --sudo \
  --flake path:/home/mirsella/dev/nixos#predator \
  --max-jobs 1 --cores 2 --no-update-lock-file
sudo journalctl -fu predator-restored-boot
```

Check that unit's `Result` and `ExecMainStatus` as above. Then reboot without
typing a passphrase and inspect:

```bash
sudo sbctl status
sudo journalctl -b -u 'systemd-cryptsetup@crypt\x2droot.service' \
  -u 'systemd-cryptsetup@fast\x2dcrypt.service'
sudo zpool status -x
sudo systemctl status tank-unlock.service sops-install-secrets.service
```

If unlock fails, use the recovery passphrase and inspect the full cryptsetup
journal. Re-enable the intended Secure Boot state before re-enrolling. Normal
Nix rebuilds do not enroll tokens, replace recovery credentials, or format disks.

## Fresh installation onto blank disks

This section creates new encrypted filesystems and destroys existing data on
the selected disks. It is not part of the preserving reinstall above. Recheck
serials and edit the device IDs in the Nix files if hardware changed.

### Root SSD and EFI partition

`disko.nix` does not manage the Hynix root disk. For an intentionally empty root
SSD, create the existing two-partition layout from the live system:

```bash
(
set -e
root_disk=/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68
parted --script "$root_disk" mklabel gpt \
  mkpart ESP fat32 1MiB 1025MiB set 1 esp on \
  mkpart root 1025MiB 100%
udevadm settle
mkfs.fat -F 32 "${root_disk}-part1"
cryptsetup luksFormat --type luks2 "${root_disk}-part2"
cryptsetup open "${root_disk}-part2" crypt-root
mkfs.ext4 /dev/mapper/crypt-root
mount /dev/mapper/crypt-root /mnt
mkdir -p /mnt/boot
mount "${root_disk}-part1" /mnt/boot
)
```

Use a strong initial recovery passphrase and record it in Proton Pass. Record
the new UUIDs with `blkid` and update `hosts/predator-hardware.nix`, especially
the EFI filesystem UUID. Review the output of `nixos-generate-config --root
/mnt --show-hardware-config`; do not overwrite the main configuration with a
generated default. On replacement hardware, also update the by-id overrides
in `hosts/predator-boot.nix` and `modules/nixos/storage.nix`.

### Data disks, only if all three are being recreated

If the Crucial SSD and HDDs already contain pools to keep, skip this subsection.
For three intentionally disposable disks, `disko.nix` creates LUKS2 containers,
the `fast` pool, and the mirrored `tank` pool. It prompts for the initial
recovery passphrases and does not add runtime keyfiles or enroll a TPM.

After placing the checkout on the mounted root and setting `repo` as above,
run the pinned Disko input explicitly:

```bash
nix run --inputs-from "$repo" disko -- \
  --mode destroy,format,mount "$repo/disko.nix"
```

This selects all three data disks listed at the top of `disko.nix`. Do not use
it for a one-disk replacement. After provisioning, export the pools before
rebooting the live environment:

```bash
umount /mnt/var/lib/nextcloud/data
zpool export fast
zpool export tank
```

Install the minimal target after restoring or creating its signing bundle.
For preserved HDDs, restore their keyfiles; for new HDDs, add keyfiles with the
`luksAddKey` procedure above, keeping the initial recovery passphrases. Restore
or authorize the SOPS identity, then restore application data before installing
the full server target. Finish Secure Boot enrollment if needed, re-enroll TPM
from the final Secure Boot-enabled boot, and verify unattended startup.
