{ config, lib, pkgs, ... }:
let
  physical = import ../../hosts/predator/disks.nix;
  mountpoint = "/srv/backup/recovery";
  dataset = config.fileSystems.${mountpoint}.device;
  partitions = lib.mapAttrs (_: device: "${device}-part1")
    (physical.hdds // { fast = physical.ssds.fast; }) // {
      root = config.boot.initrd.luks.devices.crypt-root.device;
    };
  jobs = {
    db-backup = {
      description = "Back up Nextcloud and Immich databases and Nextcloud configuration";
      startAt = "23:00";
      out = "${mountpoint}/db";
      requires = [ "postgresql.service" "podman-immich_postgres.service" ];
      path = [ pkgs.podman ];
      script = ''
        runuser -u postgres -- ${config.services.postgresql.package}/bin/pg_dump \
          --format=custom nextcloud > "$staging/nextcloud.dump"
        podman exec immich_postgres pg_dump \
          --format=custom -U immich immich > "$staging/immich.dump"
        tar --create --dereference --file="$staging/nextcloud-config.tar" \
          --directory=${lib.escapeShellArg config.services.nextcloud.home} config
      '';
    };
    recovery-backup = {
      description = "Back up LUKS headers, HDD keys, SOPS host identity and Secure Boot keys";
      startAt = "23:10";
      out = mountpoint;
      requires = [ ];
      path = [ pkgs.cryptsetup ];
      script = ''
        install -d -m 0700 "$staging/luks"
        ${lib.concatStringsSep "\n" (lib.mapAttrsToList (name: device: ''
          cryptsetup luksHeaderBackup ${lib.escapeShellArg device} --header-backup-file "$staging/luks/${name}.header"
          cryptsetup luksUUID "$staging/luks/${name}.header" > "$staging/luks/${name}.uuid"
        '') partitions)}
        tar --create --dereference --file="$staging/system-secrets.tar" --directory=/ \
          ${lib.escapeShellArgs (map (lib.removePrefix "/")
            ([ "/etc/luks" "/var/lib/sbctl" ] ++ config.sops.age.sshKeyPaths))}
      '';
    };
  };
in
{
  systemd.services = lib.mapAttrs (_: job: {
    inherit (job) description startAt requires;
    after = job.requires;
    unitConfig.RequiresMountsFor = [ mountpoint ];
    path = [ pkgs.coreutils pkgs.util-linux pkgs.gnutar ] ++ job.path;
    serviceConfig = {
      Type = "oneshot";
      UMask = "0077";
    };
    script = ''
      set -euo pipefail
      if ! findmnt --source ${lib.escapeShellArg dataset} --mountpoint ${lib.escapeShellArg mountpoint} >/dev/null; then
        echo "Backups require ${dataset} mounted at ${mountpoint}" >&2
        exit 1
      fi

      out=${lib.escapeShellArg job.out}
      install -d -o root -g root -m 0700 "$out"
      staging=$(mktemp -d "$out/.incomplete.XXXXXX")
      trap 'rm -rf -- "$staging"' EXIT
      ${job.script}

      destination="$out/backup-$(date -u +%Y%m%dT%H%M%S.%NZ)"
      mv --no-target-directory "$staging" "$destination"
      ln -s --force --no-dereference "$(basename "$destination")" "$out/.latest"
      mv --no-target-directory --force "$out/.latest" "$out/latest"
      echo "Backup completed: $destination"

      export LC_ALL=C
      shopt -s nullglob
      # Keep the published bundle even if the clock moved backwards.
      backups=()
      for backup in "$out"/backup-*/; do
        backup=''${backup%/}
        [[ "$backup" == "$destination" ]] || backups+=("$backup")
      done
      if (( ''${#backups[@]} > 13 )); then
        rm -rf -- "''${backups[@]:0:''${#backups[@]}-13}"
      fi
    '';
  }) jobs;
  systemd.timers = lib.mapAttrs (_: _: { timerConfig.Persistent = true; }) jobs;
}
