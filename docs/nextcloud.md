# Nextcloud preferences and recovery

Nextcloud is served at `https://mirsella.mooo.com/nextcloud`. The personal files
directory is `/var/lib/nextcloud/data/mirsella/files`, on the `fast/ncdata` ZFS
dataset on the Crucial 240 GB SSD. PostgreSQL and Nextcloud configuration must
be restored with the files; a filesystem-only restore does not recover accounts,
shares, two-factor enrollment or the Immich connection.

## Declarative preferences

`modules/nixos/nextcloud-preferences.nix` is imported by the Nextcloud module.
Its setup hook uses native `nextcloud-occ` commands and can run repeatedly:

- Enable public links and public uploads.
- Default public links to read-only, including links to external storage.
  Permissions remain editable; uploads and file drops are available in the UI.
- Leave passwords and expiration dates optional and off by default. There is no
  enforced expiration limit.
- Enable sharing on every configured global external mount.
- Recreate missing `Fast`, `Archive` and `TankBackup` mounts. Fast and Archive
  apply to `mirsella` and `admin`; TankBackup applies only to `admin`.
- Enable the backup-code app, without generating or replacing recovery codes.

Nextcloud hardcodes sharing off when a new external mount is created; there is
no supported global default to change. The setup hook enables it on all mounts
present when it runs. For a mount added through the UI afterward, check **Enable
sharing**, or run `sudo nextcloud-occ files_external:option ID enable_sharing true`.
Mount visibility and public-link creation are separate: enabling sharing does
not publish a folder or grant other accounts access to it.

The experimental `quota_include_external_storage = true` setting includes
external folders in the usage counter. `mirsella` has an unlimited quota.
The counter reports logical contents accessible through Nextcloud, including
shared external mounts, rather than physical ZFS allocation, snapshot usage or
bytes uploaded specifically by this account.

## Per-account state

TOTP is enabled for `mirsella`. Store its single-use recovery codes in the
**Nextcloud 2FA backup codes** custom field of the existing **nextcloud** login
entry in Proton Pass's Personal vault. Use local `pass-cli` for vault access.
The CLI creates new custom fields as text; masking a new field requires changing
its type in the Proton Pass UI. Keep the codes in the same login entry rather
than creating a separate recovery item.

Generate replacement codes in **Personal settings → Security** only when needed,
then replace that field in Proton Pass. Generation invalidates previous
codes. Nix activation must never rotate these account credentials. Restoring
the Nextcloud database restores their hashes and used/unused state; a fresh
account needs TOTP enrollment and newly generated codes saved in Proton Pass.

Create a file drop when needed from a folder's **Share** panel by choosing an
upload-only public link. Choose any password, expiry or permissions yourself.
Setup and rebuilds must not create public links, file-drop folders or passwords.

The Immich integration is configured per user under **Personal settings → Immich
Integration**, using `https://photos.mirsella.mooo.com` and a dedicated scoped
Immich API key. Restore the Nextcloud and Immich databases together to retain
that connection. A fresh account needs a new key and this connection configured.

Both workstations mount Nextcloud at `~/Documents/Nextcloud` through the
`rclone-nextcloud` user service. Credentials come from SOPS; restart the mount
after changing its app password because a running rclone retains its credentials.

Preferences: use the rclone/WebDAV mount, **no Nextcloud desktop sync
client**. Keep contacts and calendars with **Google**; do not install or migrate
Nextcloud Contacts/Calendar as part of reinstall automation.

## Reinstall

Restore the database and `nextcloud-config.tar` from the same completed nightly
backup set under `/srv/backup/db`, along with the relevant files and SOPS
identity. See [Predator reinstall](predator-reinstall.md) and
[database backup notes](immich.md). The declarative setup recreates mount
definitions and sharing policy; it does not recreate `mirsella`, existing public
links, TOTP secrets, recovery codes, or an Immich account/API key from scratch.
Create the user before using its mounts on a clean installation, then restore or
configure those account settings explicitly.
