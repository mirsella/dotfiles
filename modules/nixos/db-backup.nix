{ pkgs, config, ... }:
{
  systemd.services.db-backup = {
    description = "Dump Nextcloud and Immich databases to /srv/backup/db";
    path = with pkgs; [ postgresql podman zstd coreutils findutils ];
    script = ''
      set -eu
      out=/srv/backup/db
      mkdir -p "$out"
      stamp=$(date +%Y%m%d-%H%M)
      runuser -u postgres -- pg_dump nextcloud | zstd -T0 -o "$out/nextcloud-$stamp.sql.zst"
      PGPASSWORD=$(cat ${config.sops.secrets.immich_db_password.path}) \
        podman exec immich_postgres pg_dump -U immich immich | zstd -T0 -o "$out/immich-$stamp.sql.zst"
      ls -t "$out"/nextcloud-*.sql.zst | tail -n +8 | xargs -r rm --
      ls -t "$out"/immich-*.sql.zst | tail -n +8 | xargs -r rm --
    '';
    serviceConfig = {
      Type = "oneshot";
    };
  };

  systemd.timers.db-backup = {
    description = "Nightly database dumps for Nextcloud and Immich";
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnCalendar = "03:00";
      Persistent = true;
    };
  };
}
