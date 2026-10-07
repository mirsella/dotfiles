{
  config,
  lib,
  pkgs,
  ...
}:
let
  physical = import ../../hosts/predator/disks.nix;
  ssds = builtins.attrValues physical.ssds;
  hdds = builtins.attrValues physical.hdds;
  recipient = "mirsella@protonmail.com";
  # Sunday afternoon avoids overnight suspend and separates the HDD tests.
  hddLongHours = {
    tank1 = "14";
    tank2 = "15";
    tank3 = "13";
  };
  smartDevice = flags: longHour: device: {
    inherit device;
    options = "${flags} -s (S/../.././12|L/../../7/${longHour})";
  };
  # SMART and ZED share the Resend key used by Nextcloud/Beszel.
  mail = pkgs.writeShellApplication {
    name = "storage-resend-mail";
    text = ''
      exec ${pkgs.host-tools}/bin/host-tools storage-mail \
        ${lib.escapeShellArg config.sops.secrets.nextcloud-resend.path} \
        ${lib.escapeShellArg recipient} "$@"
    '';
  };
  alert = pkgs.writeShellApplication {
    name = "smartd-resend-alert";
    text = ''
      exec ${pkgs.host-tools}/bin/host-tools smart-alert \
        ${lib.escapeShellArg config.sops.secrets.nextcloud-resend.path} \
        ${lib.escapeShellArg recipient}
    '';
  };
in
{
  environment.systemPackages = [ pkgs.smartmontools ];

  services.smartd = {
    enable = true;
    autodetect = false;
    # Use the Resend callback instead of the module's wall notification callback.
    notifications.wall.enable = false;
    notifications.mail.enable = false;
    # Avoid toggling vendor-specific offline collection.
    # Missing data disks must not stop health monitoring of available disks.
    defaults.monitored = "-a -d sat -d removable -S on -m <nomailer> -M daily -M exec ${lib.getExe alert}";
    devices = map (smartDevice "-W 0,0,65" "13") ssds
      ++ lib.mapAttrsToList (name: device:
        smartDevice "-W 0,0,55" hddLongHours.${name} device
      ) physical.hdds;
  };

  systemd.services.smartd = {
    wants = [ "network-online.target" ];
    after = [ "network-online.target" "sops-install-secrets.service" ];
    serviceConfig = {
      Restart = "on-failure";
      RestartSec = "1min";
    };
  };

  services.zfs.zed = {
    # The callback sends directly through Resend, without a local MTA.
    enableMail = false;
    settings = {
      ZED_EMAIL_ADDR = [ recipient ];
      ZED_EMAIL_PROG = lib.getExe mail;
      ZED_EMAIL_OPTS = "'@SUBJECT@'";
      ZED_NOTIFY_DATA = true;
      ZED_NOTIFY_INTERVAL_SECS = 3600;
    };
  };
  # ZED's data handler also works for individual I/O and checksum error events.
  environment.etc = lib.genAttrs [ "zfs/zed.d/io-notify.sh" "zfs/zed.d/checksum-notify.sh" ] (_: {
    source = "${config.boot.zfs.package}/etc/zfs/zed.d/data-notify.sh";
  });
  systemd.services.zfs-zed = {
    wants = [ "network-online.target" ];
    after = [ "network-online.target" "sops-install-secrets.service" ];
    restartTriggers = [ config.environment.etc."zfs/zed.d/zed.rc".source ];
  };

  services.beszel.hub = {
    enable = true;
    environment.APP_URL = "https://mirsella.mooo.com/beszel";
    environmentFile = config.sops.secrets.beszel-heartbeat.path;
  };
  services.beszel.agent = {
    enable = true;
    environment.LISTEN = "127.0.0.1:45876";
    # The native ZFS collector discovers pools and datasets, including legacy mounts.
    extraPath = [ config.boot.zfs.package ];
    smartmon.enable = true;
    # Beszel scans whole disks; partition-only permissions do not cover them.
    smartmon.deviceAllow = ssds ++ hdds;
    environment.KEY = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIO6dTO3k45VaN7Xjt4cKpNF2Fw5TvZEoMW+pmgTjqk6A";
  };
  systemd.services.beszel-agent.serviceConfig.DeviceAllow = [ "/dev/zfs rw" ];

  sops.secrets.beszel-heartbeat = {
    sopsFile = ../../secrets/beszel.yaml;
    key = "heartbeat_env";
    restartUnits = [ "beszel-hub.service" ];
  };
  sops.secrets.beszel-superuser = {
    sopsFile = ../../secrets/beszel.yaml;
    key = "superuser_password";
  };
  sops.secrets.nextcloud-resend.restartUnits = [ "beszel-setup.service" ];

  # PocketBase has no declarative NixOS options for SMTP, users or systems.
  systemd.services.beszel-setup = {
    description = "Converge Beszel hub settings, account and system";
    wantedBy = [ "multi-user.target" ];
    requires = [
      "beszel-hub.service"
      "sops-install-secrets.service"
    ];
    after = [
      "beszel-hub.service"
      "sops-install-secrets.service"
    ];
    serviceConfig = {
      Type = "oneshot";
      # sops-nix uses try-restart, so successful setup must remain active.
      RemainAfterExit = true;
      ExecStart = "${pkgs.host-tools}/bin/host-tools beszel-setup ${config.sops.secrets.beszel-superuser.path} ${config.sops.secrets.nextcloud-resend.path}";
    };
  };
}
