{ pkgs, config, ... }:
let
  backup = pkgs.writeShellApplication {
    name = "db-backup";
    runtimeInputs = [ pkgs.coreutils pkgs.util-linux pkgs.podman pkgs.gnutar ];
    text = ''
      if ! findmnt --source tank/backup --mountpoint /srv/backup >/dev/null; then
        echo "Database backups require tank/backup mounted at /srv/backup" >&2
        exit 1
      fi

      out=/srv/backup/db
      install -d -m 0700 "$out"
      staging=$(mktemp -d "$out/.incomplete.XXXXXX")
      trap 'rm -rf -- "$staging"' EXIT

      runuser -u postgres -- ${config.services.postgresql.package}/bin/pg_dump \
        --format=custom nextcloud > "$staging/nextcloud.dump"
      podman exec immich_postgres pg_dump \
        --format=custom -U immich immich > "$staging/immich.dump"
      tar --create --dereference --file="$staging/nextcloud-config.tar" \
        --directory=${config.services.nextcloud.home} config

      destination="$out/backup-$(date -u +%Y%m%dT%H%M%S.%NZ)"
      mv --no-target-directory "$staging" "$destination"
      echo "Database backup completed: $destination"

      export LC_ALL=C
      shopt -s nullglob
      backups=("$out"/backup-*)
      if (( ''${#backups[@]} > 14 )); then
        rm -rf -- "''${backups[@]:0:''${#backups[@]}-14}"
      fi
    '';
  };
in
{
  systemd.services.db-backup = {
    description = "Back up Nextcloud and Immich databases and Nextcloud configuration";
    requires = [ "postgresql.service" "podman-immich_postgres.service" "zfs-mount.service" ];
    after = [ "postgresql.service" "podman-immich_postgres.service" "zfs-mount.service" ];
    unitConfig.RequiresMountsFor = [ "/srv/backup" ];
    startAt = "23:00";
    serviceConfig = {
      Type = "oneshot";
      UMask = "0077";
      ExecStart = "${backup}/bin/db-backup";
    };
  };

  systemd.timers.db-backup.timerConfig.Persistent = true;
}
