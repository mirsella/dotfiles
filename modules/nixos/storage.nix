{
  pkgs,
  lib,
  config,
  ...
}:
{
  boot.zfs.extraPools = [
    "fast"
    "tank"
  ];

  boot.initrd.luks.devices = {
    fast-crypt = {
      device = "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68-part1";
      crypttabExtraOpts = [ "tpm2-device=auto" ];
      allowDiscards = true;
    };
  };

  systemd.services.zfs-import-tank = {
    after = [ "tank-unlock.service" ];
    requires = [ "tank-unlock.service" ];
  };

  systemd.services.tank-unlock = {
    description = "Unlock tank disks after USB initialization";
    after = [ "systemd-udevd.service" ];
    path = [
      pkgs.cryptsetup
      config.systemd.package
    ];
    restartIfChanged = false;
    unitConfig.DefaultDependencies = false;
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
    };
    script = ''
      unlock() {
        udevadm wait --timeout=360 --initialized=yes "$3"
        for attempt in {1..12}; do
          cryptsetup open --key-file "$2" "$3" "$1" && return 0
          (( attempt < 12 )) || return 1
          sleep 5
        done
      }
      unlock tank1-crypt /etc/luks/tank1.key /dev/disk/by-uuid/e2eef868-fc6e-4f7b-864d-8c8235c7df37
      unlock tank2-crypt /etc/luks/tank2.key /dev/disk/by-uuid/16c6cbcd-79fd-4dbd-bdfe-fc94ef3d3221
    '';
  };

  fileSystems."/var/lib/nextcloud/data" = {
    device = "fast/ncdata";
    fsType = "zfs";
    options = [ "nofail" ];
  };

  services.zfs = {
    autoScrub = {
      enable = true;
      interval = "*-*-01 10:00";
      randomizedDelaySec = "30min";
      pools = [
        "fast"
        "tank"
      ];
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
      autosnap = true;
      autoprune = true;
    };
    datasets = lib.genAttrs [ "fast/ncdata" "fast/data" "tank/archive" "tank/backup" ] (_: {
      use_template = [ "keep" ];
      recursive = true;
    });
  };
  systemd.services.sanoid = {
    requires = [ "zfs-mount.service" ];
    after = [
      "zfs-mount.service"
      "db-backup.service"
    ];
  };
  systemd.timers.sanoid.timerConfig.Persistent = true;
}
