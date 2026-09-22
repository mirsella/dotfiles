# Immich on Predator

`modules/nixos/immich.nix` defines the four Podman containers, image digests,
database credentials, storage dependencies, health checks, and system settings.
Caddy serves `https://photos.mirsella.mooo.com` through `127.0.0.1:2283`.

## Configuration

Edit `configFile` in the module for Immich settings. `IMMICH_CONFIG_FILE` makes
the admin settings page read-only. Omitted settings use the pinned version's
defaults. Account management still uses the web UI.

SOPS decrypts `db_password` from `secrets/immich.yaml` before PostgreSQL and
Immich start. Both containers read the same read-only secret file. PostgreSQL
uses `POSTGRES_PASSWORD_FILE`; Immich uses `DB_PASSWORD_FILE`. A populated
PostgreSQL database retains its role password, so changing the encrypted secret
also requires changing the database role password.

For upgrades, change the server and machine-learning version and both image
digests together, then check the release's Compose file for database and Valkey
changes. Build and switch the `predator` flake on Predator with its configured
build limits. The deployed checkout is currently `~/dev/nixos`.

## Persistent state

| State | Location |
| --- | --- |
| PostgreSQL, including accounts and API keys | `/var/lib/immich-pg`, UID/GID 999 |
| Originals, generated media and profiles | `/srv/data/fast/Photos` on `fast/data` |
| Downloadable ML models | Podman volume `immich-model-cache` |

Immich waits for the ZFS mount and a healthy PostgreSQL/Valkey pair. It creates
the library directories and `.immich` markers on first initialization. Missing
markers in an existing library remain errors; restoring the correct media tree
is part of restoring the service.

On a fresh installation, mount the storage and provide a SOPS decryption key,
then activate the configuration and create the first admin in the web UI.
To preserve an existing installation, restore its database and entire media
tree, including hidden files, before starting the server.

## Backups

`db-backup.service` runs at 23:00 in the host timezone, catches up after downtime,
and inhibits sleep while running. Immich's internal backup scheduler is disabled.
The job retains 14 timestamped directories on `tank/backup` under `/srv/backup/db`.
Each contains `immich.dump`, `nextcloud.dump`, and `nextcloud-config.tar`.

A backup becomes visible only after both dumps and the configuration archive
succeed. A dump or archive failure leaves completed backups untouched. The directory is
root-only because Nextcloud's configuration includes its instance secrets.

Restore the compressed custom-format dumps with `pg_restore` from the matching
PostgreSQL major version. Keep the media, encrypted secrets and decryption keys
in an off-machine backup as well. Follow the matching Immich version's
[restore procedure](https://docs.immich.app/administration/backup-and-restore/).

## Diagnosis

```sh
sudo systemctl status sops-install-secrets podman-immich_{postgres,redis,server,ml}
sudo journalctl -u podman-immich_server -b
sudo podman ps --format '{{.Names}} {{.Status}}'
curl --fail https://photos.mirsella.mooo.com/api/server/ping
```

Restart containers through their systemd services. NixOS recreates the
containers with the configured mounts and environment when those services start.
