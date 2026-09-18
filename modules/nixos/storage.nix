{
  pkgs,
  lib,
  ...
}:
let
  snapPolicy = ''
    [template_keep]
    daily = 7
    weekly = 4
    autosnap = yes
    autoprune = yes
  '';
  mkConditionalSanoid = name: dataset:
    let
      confDir = pkgs.writeTextDir "sanoid.conf" ''
        ${snapPolicy}
        [${dataset}]
        use_template = keep
        recursive = yes
      '';
      sanoidRun = "${pkgs.sanoid}/bin/sanoid --cron --configdir ${confDir} --cache-dir /var/cache/sanoid-${name} --run-dir /run/sanoid-${name}";
      daily = pkgs.writeShellScript "sanoid-if-dirty-${name}" ''
        if ! systemctl is-active --quiet zfs-dirty-flag.service; then
          echo "dirty-flag daemon not active, snapshotting ${dataset} to be safe"
          rm -f /var/lib/sanoid-gate/${name}.dirty
          exec ${sanoidRun}
        fi
        flag=/var/lib/sanoid-gate/${name}.dirty
        if [ ! -e "$flag" ]; then
          echo "no writes flagged for ${dataset}, skipping snapshots"
          exit 0
        fi
        changed=0
        for d in $(${pkgs.zfs}/bin/zfs list -r -H -o name ${dataset}); do
          latest=$(${pkgs.zfs}/bin/zfs list -t snapshot -H -o name -S creation "$d" | grep '@autosnap_' | head -1)
          if [ -z "$latest" ]; then changed=1; break; fi
          if [ -n "$(${pkgs.zfs}/bin/zfs diff "$latest" "$d" 2>/dev/null | head -1)" ]; then changed=1; break; fi
        done
        if [ "$changed" = 0 ]; then
          rm -f "$flag"
          echo "flag stale for ${dataset}, cleared without snapshotting"
          exit 0
        fi
        before=$(${pkgs.zfs}/bin/zfs list -t snapshot -H -o name)
        ${sanoidRun}
        after=$(${pkgs.zfs}/bin/zfs list -t snapshot -H -o name)
        if [ -n "$(comm -13 <(echo "$before") <(echo "$after") | head -1)" ]; then
          rm -f "$flag"
          echo "snapshotted ${dataset}, flag cleared"
        else
          echo "snapshot not due yet for ${dataset}, flag kept for next run"
        fi
      '';
      verify = pkgs.writeShellScript "sanoid-verify-${name}" ''
        changed=0
        for d in $(${pkgs.zfs}/bin/zfs list -r -H -o name ${dataset}); do
          latest=$(${pkgs.zfs}/bin/zfs list -t snapshot -H -o name -S creation "$d" | grep '@autosnap_' | head -1)
          if [ -z "$latest" ]; then changed=1; break; fi
          if [ -n "$(${pkgs.zfs}/bin/zfs diff "$latest" "$d" 2>/dev/null | head -1)" ]; then changed=1; break; fi
        done
        if [ "$changed" = 1 ]; then
          exec ${sanoidRun}
        else
          echo "verify: no changes on ${dataset}, skipping snapshots"
        fi
      '';
    in
    {
      systemd.services."sanoid-${name}" = {
        description = "Snapshot ${dataset} if flagged dirty";
        after = [ "zfs-dirty-flag.service" ];
        serviceConfig = {
          Type = "oneshot";
          ExecStart = daily;
          CacheDirectory = "sanoid-${name}";
          RuntimeDirectory = "sanoid-${name}";
        };
      };
      systemd.timers."sanoid-${name}" = {
        wantedBy = [ "timers.target" ];
        timerConfig = {
          OnCalendar = "daily";
          Persistent = true;
          RandomizedDelaySec = "1h";
        };
      };
      systemd.services."sanoid-${name}-verify" = {
        description = "Weekly backstop: snapshot ${dataset} if diff shows changes";
        serviceConfig = {
          Type = "oneshot";
          ExecStart = verify;
          CacheDirectory = "sanoid-${name}";
          RuntimeDirectory = "sanoid-${name}";
        };
      };
      systemd.timers."sanoid-${name}-verify" = {
        wantedBy = [ "timers.target" ];
        timerConfig = {
          OnCalendar = "weekly";
          Persistent = true;
          RandomizedDelaySec = "6h";
        };
      };
    };
  dirtyFlagDaemon = {
    systemd.services.zfs-dirty-flag = {
      description = "Flag ZFS dataset writes for conditional snapshots";
      wantedBy = [ "multi-user.target" ];
      after = [ "zfs-import-tank.service" "zfs-import-fast.service" ];
      serviceConfig = {
        Type = "simple";
        ExecStart = "${pkgs.python3}/bin/python3 ${./dirty-flag.py} /var/lib/sanoid-gate";
        StateDirectory = "sanoid-gate";
        Restart = "always";
        RestartSec = "5s";
      };
    };
  };
in
lib.mkMerge [
  {
    networking.hostId = "007f0200";
  boot.supportedFilesystems = [ "zfs" ];
  boot.zfs.forceImportRoot = false;
  boot.zfs.extraPools = [ "fast" "tank" ];

  fileSystems."/var/lib/nextcloud/data" = {
    device = "fast/ncdata";
    fsType = "zfs";
    options = [ "nofail" ];
  };

  services.zfs = {
    autoScrub = {
      enable = true;
      interval = "monthly";
      pools = [ "fast" "tank" ];
    };
    trim = {
      enable = true;
      interval = "weekly";
    };
  };

  environment.systemPackages = with pkgs; [ smartmontools ];

  services.smartd = {
    enable = true;
    autodetect = false;
    devices = [
      { device = "/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68"; }
      { device = "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68"; }
      {
        device = "/dev/disk/by-id/wwn-0x5000c500aa3cc143";
        options = "-d sat -n standby,q";
      }
      {
        device = "/dev/disk/by-id/wwn-0x500003961228993f";
        options = "-d sat -n standby,q -c interval=43200";
      }
    ];
  };

  systemd.services.hd-idle = {
    description = "Spin down idle tank HDDs";
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      ExecStart = "${pkgs.hd-idle}/bin/hd-idle -i 0 -a /dev/disk/by-id/wwn-0x5000c500aa3cc143 -i 2700 -c scsi -a /dev/disk/by-id/wwn-0x500003961228993f -i 2700 -c scsi";
    };
  };
  }

  (mkConditionalSanoid "tank" "tank/library")
  (mkConditionalSanoid "backup" "tank/backup")
  (mkConditionalSanoid "fast" "fast/ncdata")
  dirtyFlagDaemon
]
