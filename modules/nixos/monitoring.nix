{
  config,
  lib,
  pkgs,
  ...
}:
let
  disks = [
    "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68"
    "/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68"
    "/dev/disk/by-id/wwn-0x500003961228993f"
    "/dev/disk/by-id/wwn-0x5000c500aa3cc143"
  ];
  # Failure mail via the Resend key shared with Nextcloud/Beszel.
  alert = pkgs.writeShellApplication {
    name = "smartd-resend-alert";
    runtimeInputs = [
      pkgs.curl
      pkgs.jq
    ];
    text = ''
      key=$(<${config.sops.secrets.nextcloud-resend.path})
      payload=$(jq -n \
        --arg from 'SMART <noreply@voxride.com>' \
        --arg to 'mirsella@protonmail.com' \
        --arg subject "$SMARTD_SUBJECT" \
        --arg text "$SMARTD_FULLMESSAGE" \
        '{from:$from,to:[$to],subject:$subject,text:$text}')
      curl --fail-with-body -sS --max-time 30 https://api.resend.com/emails \
        -H "Authorization: Bearer $key" \
        -H 'Content-Type: application/json' \
        --data "$payload"
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
    # Schedule short self-tests without toggling vendor-specific offline collection.
    defaults.monitored = "-a -d sat -S on -s (S/../.././12) -m <nomailer> -M once -M exec ${lib.getExe alert}";
    devices = map (device: { inherit device; }) disks;
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
    smartmon.deviceAllow = disks;
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
    };
    script = ''
      ${pkgs.python3}/bin/python3 ${./beszel-setup.py} \
        ${config.sops.secrets.beszel-superuser.path} \
        ${config.sops.secrets.nextcloud-resend.path}
    '';
  };
}
