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
      check = pkgs.writeShellScript "sanoid-if-changed-${name}" ''
        changed=0
        for d in $(${pkgs.zfs}/bin/zfs list -r -H -o name ${dataset}); do
          latest=$(${pkgs.zfs}/bin/zfs list -t snapshot -H -o name -S creation "$d" | grep '@autosnap_' | head -1)
          if [ -z "$latest" ]; then changed=1; break; fi
          if [ "$(${pkgs.zfs}/bin/zfs get -H -p -o value written "$latest")" != 0 ]; then changed=1; break; fi
        done
        if [ "$changed" = 1 ]; then
          exec ${pkgs.sanoid}/bin/sanoid --cron --configdir ${confDir} --cache-dir /var/cache/sanoid-${name} --run-dir /run/sanoid-${name}
        else
          echo "no changes on ${dataset}, skipping snapshots"
        fi
      '';
    in
    {
      systemd.services."sanoid-${name}" = {
        description = "Snapshot ${dataset} only if changed";
        serviceConfig = {
          Type = "oneshot";
          ExecStart = check;
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
  (mkConditionalSanoid "fast" "fast/ncdata")
]
