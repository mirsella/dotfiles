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
      directories = [ ];
      commands = [
        {
          program = "${pkgs.util-linux}/bin/runuser";
          args = [ "-u" "postgres" "--" "${config.services.postgresql.package}/bin/pg_dump" "--format=custom" "nextcloud" ];
          stdout = "nextcloud.dump";
        }
        {
          program = "${pkgs.podman}/bin/podman";
          args = [ "exec" "immich_postgres" "pg_dump" "--format=custom" "-U" "immich" "immich" ];
          stdout = "immich.dump";
        }
        {
          program = "${pkgs.gnutar}/bin/tar";
          args = [ "--create" "--dereference" "--file=nextcloud-config.tar" "--directory=${config.services.nextcloud.home}" "config" ];
        }
      ];
    };
    recovery-backup = {
      description = "Back up LUKS headers, HDD keys, SOPS host identity and Secure Boot keys";
      startAt = "23:10";
      out = mountpoint;
      requires = [ ];
      directories = [ "luks" ];
      commands = lib.concatLists (lib.mapAttrsToList (name: device: [
        {
          program = "${pkgs.cryptsetup}/bin/cryptsetup";
          args = [ "luksHeaderBackup" device "--header-backup-file" "luks/${name}.header" ];
        }
        {
          program = "${pkgs.cryptsetup}/bin/cryptsetup";
          args = [ "luksUUID" "luks/${name}.header" ];
          stdout = "luks/${name}.uuid";
        }
      ]) partitions) ++ [
        {
          program = "${pkgs.gnutar}/bin/tar";
          args = [ "--create" "--dereference" "--file=system-secrets.tar" "--directory=/" ]
            ++ map (lib.removePrefix "/") ([ "/etc/luks" "/var/lib/sbctl" ] ++ config.sops.age.sshKeyPaths);
        }
      ];
    };
  };
in
{
  systemd.services = lib.mapAttrs (name: job: {
    inherit (job) description startAt requires;
    after = job.requires;
    unitConfig.RequiresMountsFor = [ mountpoint ];
    path = [ pkgs.util-linux ];
    serviceConfig = {
      Type = "oneshot";
      UMask = "0077";
      ExecStart = "${pkgs.host-tools}/bin/host-tools backup ${pkgs.writers.writeJSON "${name}.json" {
        inherit dataset mountpoint;
        output = job.out;
        inherit (job) commands directories;
      }}";
    };
  }) jobs;
  systemd.timers = lib.mapAttrs (_: _: { timerConfig.Persistent = true; }) jobs;
}
