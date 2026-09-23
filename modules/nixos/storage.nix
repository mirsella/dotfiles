{
  pkgs,
  lib,
  config,
  ...
}:
{
  boot.zfs.extraPools = [ "fast" "tank" ];

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
    path = [ pkgs.cryptsetup config.systemd.package ];
    unitConfig.DefaultDependencies = false;
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
    };
    script = ''
      unlock() {
        cryptsetup status "$1" >/dev/null 2>&1 && return 0
        udevadm wait --timeout=360 --initialized=yes "$3"
        for attempt in {1..12}; do
          cryptsetup open --key-file "$2" "$3" "$1" && return 0
          (( attempt < 12 )) || return 1
          sleep 5
        done
      }
      unlock tank1-crypt /etc/luks/tank1.key /dev/disk/by-id/wwn-0x5000c500aa3cc143-part1
      unlock tank2-crypt /etc/luks/tank2.key /dev/disk/by-id/wwn-0x500003961228993f-part1
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
      pools = [ "fast" "tank" ];
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
    after = [ "zfs-mount.service" "db-backup.service" ];
  };
  systemd.timers.sanoid.timerConfig.Persistent = true;

  environment.systemPackages = with pkgs; [ smartmontools ];

  services.smartd =
    let
      # Failure mail via the Resend key shared with Nextcloud/Beszel.
      # smartd runs this as root with SMARTD_* env vars on failure events.
      alert = pkgs.writeShellScript "smartd-resend-alert" ''
        set -eu
        key=$(cat ${config.sops.secrets.nextcloud-resend.path})
        subject="SMART ''${SMARTD_FAILTYPE:-alert}: ''${SMARTD_DEVICE:-unknown} on predator"
        body=$(printf '%s\n' "SMART event on predator at $(date -Is)" "" "Device: ''${SMARTD_DEVICE:-?}" "Type: ''${SMARTD_FAILTYPE:-?}" "" "''${SMARTD_MESSAGE:-}")
        payload=$(${pkgs.jq}/bin/jq -n \
          --arg from 'SMART <noreply@voxride.com>' \
          --arg to 'mirsella@protonmail.com' \
          --arg subject "$subject" \
          --arg text "$body" \
          '{from:$from,to:[$to],subject:$subject,text:$text}')
        ${pkgs.curl}/bin/curl --fail-with-body -sS --max-time 30 https://api.resend.com/emails \
          -H "Authorization: Bearer $key" \
          -H 'Content-Type: application/json' \
          --data "$payload"
      '';
      # Run short self-tests at noon.
      monitored = "-a -o on -S on -s (S/../.././12) -M once -M exec ${alert}";
    in
    {
      enable = true;
      autodetect = false;
      defaults.monitored = monitored;
      devices = [
        { device = "/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68"; }
        { device = "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68"; }
        {
          device = "/dev/disk/by-id/wwn-0x5000c500aa3cc143";
          options = "-d sat";
        }
        {
          device = "/dev/disk/by-id/wwn-0x500003961228993f";
          options = "-d sat";
        }
      ];
    };

  # Genesys Logic hub autosuspends with delay 0 and takes the pool disks
  # off the bus with it (killed 6 SMART long tests). Keep it awake.
  services.udev.extraRules = ''
    ACTION=="add", SUBSYSTEM=="usb", ATTR{idVendor}=="05e3", ATTR{idProduct}=="0626", ATTR{power/control}="on"
  '';

}
