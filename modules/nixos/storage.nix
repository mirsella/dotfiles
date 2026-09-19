{
  pkgs,
  lib,
  ...
}:
let
  snapPolicy = ''
    [template_keep]
    hourly = 0
    daily = 7
    weekly = 4
    monthly = 0
    autosnap = yes
    autoprune = yes
  '';
  zfsChanged = pkgs.writeShellScript "zfs-changed" ''
    dataset=$1
    for d in $(${pkgs.zfs}/bin/zfs list -r -H -o name "$dataset"); do
      latest=$(${pkgs.zfs}/bin/zfs list -t snapshot -H -o name -S creation "$d" | grep '@autosnap_' | head -1)
      [ -z "$latest" ] && exit 0
      ${pkgs.zfs}/bin/zfs diff "$latest" "$d" 2>/dev/null | head -1 | grep -q . && exit 0
    done
    exit 1
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
        if ! ${zfsChanged} ${dataset}; then
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
        if ${zfsChanged} ${dataset}; then
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
        ExecStart = "${pkgs.zfs-dirty-flag}/bin/zfs-dirty-flag /var/lib/sanoid-gate";
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

  # Genesys Logic hub autosuspends with delay 0 and takes the pool disks
  # off the bus with it (killed 6 SMART long tests). Keep it awake.
  services.udev.extraRules = ''
    ACTION=="add", SUBSYSTEM=="usb", ATTR{idVendor}=="05e3", ATTR{idProduct}=="0626", ATTR{power/control}="on"
  '';

  systemd.services.hd-idle = {
    description = "Spin down idle tank HDDs";
    wantedBy = [ "multi-user.target" ];
    serviceConfig.ExecStart = "${pkgs.hd-idle}/bin/hd-idle -i 0 -a /dev/disk/by-id/wwn-0x5000c500aa3cc143 -i 2700 -c scsi -a /dev/disk/by-id/wwn-0x500003961228993f -i 2700 -c scsi -l /var/log/hd-idle.log";
  };

  systemd.services.hdd-activity-log = {
    description = "Log tank HDD activity and wakeups without waking disks";
    serviceConfig = {
      Type = "oneshot";
      StateDirectory = "hdd-sleep";
      ExecStart = pkgs.writeShellScript "hdd-activity-log" ''
        for id in wwn-0x5000c500aa3cc143 wwn-0x500003961228993f; do
          dev=$(readlink "/dev/disk/by-id/$id" 2>/dev/null) || continue
          sd=$(basename "$dev")
          stat="/sys/block/$sd/stat"
          [ -r "$stat" ] || continue
          read -r _ _ rsec _ _ _ wsec _ < "$stat"
          state="/var/lib/hdd-sleep/$id"
          now=$(date +%s)
          if [ -f "$state" ]; then
            read -r prsec pwsec lastActive < "$state"
            if [ "$rsec" != "$prsec" ] || [ "$wsec" != "$pwsec" ]; then
              quiet=$(( (now - lastActive) / 60 ))
              if [ "$quiet" -ge 15 ]; then
                echo "WAKE $id after $quiet min quiet (+$((rsec - prsec))r +$((wsec - pwsec))w sectors)"
              else
                echo "activity $id (+$((rsec - prsec))r +$((wsec - pwsec))w sectors)"
              fi
              echo "$rsec $wsec $now" > "$state"
            fi
          else
            echo "$rsec $wsec $now" > "$state"
            echo "tracking $id ($sd) r=$rsec w=$wsec"
          fi
        done
      '';
    };
  };
  systemd.timers.hdd-activity-log = {
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnCalendar = "*:0/5";
      Persistent = true;
    };
  };
  }

  (mkConditionalSanoid "tank" "tank/library")
  (mkConditionalSanoid "backup" "tank/backup")
  (mkConditionalSanoid "fast" "fast/ncdata")
  dirtyFlagDaemon
]
