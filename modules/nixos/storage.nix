{
  lib,
  config,
  utils,
  ...
}:
let
  physical = import ../../hosts/predator/disks.nix;
  pools = (import ../../disko.nix).disko.devices.zpool;
  replicated = builtins.attrNames pools.fast.datasets;
  unlockUnit = name: "systemd-cryptsetup@${utils.escapeSystemdPath "${name}-crypt"}.service";
  tankUnlocks = map unlockUnit (builtins.attrNames physical.hdds);
in
{
  boot.zfs.extraPools = builtins.attrNames pools;

  # Only the OS disk belongs in initrd; optional data must not block root boot.
  environment.etc.crypttab.text = ''
    fast-crypt ${physical.ssds.fast}-part1 - tpm2-device=auto,discard,nofail,headless=true,x-systemd.device-timeout=30s
  '' + lib.concatStringsSep "\n" (lib.mapAttrsToList (name: device:
    "${name}-crypt ${device}-part1 /etc/luks/${name}.key nofail,headless=true,x-systemd.device-timeout=30s"
  ) physical.hdds) + "\n";
  systemd.services."systemd-cryptsetup@" = {
    overrideStrategy = "asDropin";
    serviceConfig.TimeoutSec = "2min";
  };
  systemd.services.zfs-import-fast = {
    after = [ (unlockUnit "fast") ];
    requires = [ (unlockUnit "fast") ];
  };

  systemd.services.zfs-import-tank = {
    after = tankUnlocks;
    wants = tankUnlocks;
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

  # Snapshot unchanged datasets too: ZFS shares their blocks, and correctness
  # must not depend on an inotify watch surviving every rename and reboot.
  services.sanoid = {
    enable = true;
    interval = "23:30";
    templates.keep = {
      hourly = 0;
      daily = 7;
      weekly = 4;
      monthly = 0;
      yearly = 0;
      autoprune = true;
    };
    datasets = lib.genAttrs (map (name: "fast/${name}") replicated ++ [ "tank/archive" "tank/backup" "tank/replica" ]) (name: {
      use_template = [ "keep" ];
      recursive = true;
      autosnap = name != "tank/replica";
    });
  };
  systemd.services.sanoid = {
    after = [
      "zfs-import.target"
      "db-backup.service"
    ];
  };
  systemd.timers.sanoid.timerConfig.Persistent = true;

  services.syncoid = {
    enable = true;
    interval = "23:45";
    commonArgs = [ "--no-rollback" ];
    localTargetAllow = lib.mkOptionDefault [ "readonly" "canmount" "acltype" "aclinherit" "destroy" ];
    commands = lib.genAttrs replicated (name: {
      source = "fast/${name}";
      target = "tank/replica/${name}";
      recursive = true;
      sendOptions = "Lc p";
      recvOptions = "u o mountpoint=none o canmount=off o readonly=on";
    });
    service = {
      requires = [ "zfs-import-fast.service" "zfs-import-tank.service" ];
      after = [ "zfs-import-fast.service" "zfs-import-tank.service" "sanoid.service" ];
    };
  };
}
