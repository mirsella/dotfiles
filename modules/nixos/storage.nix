{
  lib,
  config,
  utils,
  ...
}:
let
  physical = import ../../hosts/predator/disks.nix;
  pools = (import ../../disko.nix).disko.devices.zpool;
  fastDatasets = builtins.attrNames pools.fast.datasets;
  fastDatasetNames = map (name: "fast/${name}") fastDatasets;
  fastMountpoints = map (name: pools.fast.datasets.${name}.mountpoint) fastDatasets;
  resticBackup = config.services.restic.backups.fast;
  zfs = "${config.boot.zfs.package}/bin/zfs";
  zpool = "${config.boot.zfs.package}/bin/zpool";
  unlockUnit = name: "systemd-cryptsetup@${utils.escapeSystemdPath "${name}-crypt"}.service";
  tankUnlocks = map unlockUnit (builtins.attrNames physical.hdds);
  backupUnlocks = map unlockUnit (builtins.attrNames physical.backupHdds);
  poolLifecycle = pool: {
    # Reverse the import ordering at shutdown: unmount, export, then detach LUKS.
    # A live configuration switch must not export pools beneath running writers.
    restartIfChanged = false;
    stopIfChanged = false;
    serviceConfig.ExecStop = "${zpool} export ${lib.escapeShellArg pool}";
  };
in
{
  sops.secrets."restic-password" = {
    sopsFile = ../../secrets/restic.yaml;
  };

  boot.zfs.extraPools = builtins.attrNames pools;
  # ZFS lives inside LUKS, not on the raw disks enumerated by by-id. Discover
  # decrypted devices by their stable crypttab mapper names after unlock.
  boot.zfs.pools = lib.genAttrs (builtins.attrNames pools) (_: {
    devNodes = "/dev/mapper";
  });

  systemd.services.zfs-import-fast = poolLifecycle "fast" // {
    after = [ (unlockUnit "fast") ];
    requires = [ (unlockUnit "fast") ];
  };

  systemd.services.zfs-import-tank = poolLifecycle "tank" // {
    after = tankUnlocks;
    wants = tankUnlocks;
  };
  systemd.services.zfs-import-backups = poolLifecycle "backups" // {
    after = backupUnlocks;
    wants = backupUnlocks;
  };

  fileSystems = builtins.listToAttrs (lib.concatLists (lib.mapAttrsToList (pool: spec:
    lib.mapAttrsToList (name: dataset: lib.nameValuePair dataset.mountpoint {
      device = "${pool}/${name}";
      fsType = "zfs";
      options = dataset.mountOptions;
    }) (lib.filterAttrs (_: dataset: dataset ? mountpoint) spec.datasets)
  ) pools));

  services.zfs = {
    autoScrub = {
      enable = true;
      interval = "*-*-01 10:00";
      randomizedDelaySec = "30min";
      pools = config.boot.zfs.extraPools;
    };
    trim = {
      enable = true;
      interval = "Sun 11:30";
      randomizedDelaySec = "15min";
    };
  };

  # Snapshot unchanged datasets too: ZFS shares their blocks.
  services.sanoid = {
    enable = true;
    interval = "23:30";
    templates.keep = {
      hourly = 0;
      daily = 7;
      weekly = 2;
      monthly = 0;
      yearly = 0;
      autoprune = true;
    };
    datasets = lib.genAttrs (fastDatasetNames ++ [ "tank/archive" "backups/data" ]) (_: {
      autosnap = true;
      use_template = [ "keep" ];
      recursive = true;
    });
  };
  systemd.services.sanoid = {
    after = [
      "zfs-import.target"
      "db-backup.service"
    ];
    unitConfig.OnSuccess = [ "healthchecks-ping@sanoid.service" ];
  };
  systemd.timers.sanoid.timerConfig.Persistent = true;

  # Nightly restic backup of the fast pool to the separate backups pool:
  # retention spans months, restore is file-level, and the backup runs
  # against a throwaway snapshot instead of the live tree.
  services.restic.backups.fast = {
    initialize = true;
    repository = "/srv/backup/restic";
    passwordFile = config.sops.secrets."restic-password".path;
    paths = map (mountpoint: "${mountpoint}/.zfs/snapshot/restic-backup") fastMountpoints;
    pruneOpts = [
      "--keep-daily 7"
      "--keep-weekly 8"
      "--keep-monthly 6"
    ];
    backupPrepareCommand = lib.concatMapStrings (dataset: ''
      ${zfs} destroy -r ${dataset}@restic-backup 2>/dev/null || true
      ${zfs} snapshot ${dataset}@restic-backup
    '') fastDatasetNames;
    backupCleanupCommand = lib.concatMapStrings (dataset: ''
      ${zfs} destroy -r ${dataset}@restic-backup 2>/dev/null || true
    '') fastDatasetNames;
    timerConfig = {
      OnCalendar = "23:50";
      Persistent = true;
    };
  };
  systemd.services."restic-backups-fast" = {
    requires = [ "zfs-import-fast.service" "zfs-import-backups.service" ];
    after = [ "zfs-import-fast.service" "zfs-import-backups.service" ];
    unitConfig = {
      RequiresMountsFor = [ "/srv/backup" ] ++ fastMountpoints;
      OnSuccess = [ "healthchecks-ping@restic-backup.service" ];
    };
  };

  # The module marks its timer X-OnlyManualStart, so `nixos-rebuild switch`
  # never starts it; enable it like any other timer.
  systemd.timers."restic-backups-fast".unitConfig.X-OnlyManualStart = lib.mkForce false;

  # Monthly sampled read: a successful backup write does not prove the
  # repository is intact.
  systemd.services.restic-check = {
    description = "Verify restic repository integrity (sampled data read)";
    requires = [ "zfs-import-backups.service" ];
    after = [ "zfs-import-backups.service" ];
    unitConfig = {
      RequiresMountsFor = [ "/srv/backup" ];
      OnSuccess = [ "healthchecks-ping@restic-check.service" ];
    };
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${resticBackup.package}/bin/restic -r ${resticBackup.repository} --password-file ${resticBackup.passwordFile} check --read-data-subset=10%%";
    };
  };
  systemd.timers.restic-check = {
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnCalendar = "*-*-05 05:00";
      Persistent = true;
    };
  };
}
