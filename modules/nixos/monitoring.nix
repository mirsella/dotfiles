{ config, lib, pkgs, ... }:
let
  sataDisks = [
    "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68"
    "/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68"
  ];
  # Leave the unstable Seagate bridge out of SMART polling.
  usbDisk = "/dev/disk/by-uuid/16c6cbcd-79fd-4dbd-bdfe-fc94ef3d3221";
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
in
{
  environment.systemPackages = [ pkgs.smartmontools ];

  services.smartd = {
    enable = true;
    autodetect = false;
    # Run short self-tests at noon.
    defaults.monitored = "-a -o on -S on -s (S/../.././12) -M once -M exec ${alert}";
    devices = map (device: { inherit device; }) sataDisks ++ [
      { device = usbDisk; options = "-d sat"; }
    ];
  };

  services.beszel.hub = {
    enable = true;
    environment.APP_URL = "https://mirsella.mooo.com/beszel";
    environmentFile = config.sops.secrets.beszel-heartbeat.path;
  };
  services.beszel.agent = {
    enable = true;
    environment.LISTEN = "127.0.0.1:45876";
    extraPath = [ config.boot.zfs.package ];
    # Beszel uses zfs list for these dataset mounts.
    environment.EXTRA_FILESYSTEMS = "/srv/data/fast__Fast-SSD,/srv/data/archive__Archive-HDD,/var/lib/nextcloud/data__Nextcloud";
    smartmon.enable = true;
    # /dev/zfs also needs an allow rule below: DeviceAllow closes the cgroup
    # to unlisted devices. USB bridges need explicit SAT passthrough.
    smartmon.deviceAllow = sataDisks ++ [ usbDisk ];
    environment.SMART_DEVICES = lib.concatStringsSep "," (sataDisks ++ [ "${usbDisk}:sat" ]);
    environment.KEY = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIO6dTO3k45VaN7Xjt4cKpNF2Fw5TvZEoMW+pmgTjqk6A";
  };
  systemd.services.beszel-agent.serviceConfig = {
    # The upstream module already opens /dev and grants SMART capabilities.
    DeviceAllow = [ "/dev/zfs rw" ];
    # /var/lib/nextcloud is 0750 nextcloud:nextcloud.
    SupplementaryGroups = [ "nextcloud" ];
  };

  sops.secrets.beszel-heartbeat = {
    sopsFile = ../../secrets/beszel.yaml;
    key = "heartbeat_env";
    restartUnits = [ "beszel-hub.service" ];
  };
  sops.secrets.beszel-superuser = {
    sopsFile = ../../secrets/beszel.yaml;
    key = "superuser_password";
  };

  # PocketBase has no declarative NixOS options for SMTP, users or systems.
  systemd.services.beszel-setup = {
    description = "Converge Beszel hub settings, account and system";
    wantedBy = [ "multi-user.target" ];
    requires = [ "beszel-hub.service" "sops-install-secrets.service" ];
    after = [ "beszel-hub.service" "sops-install-secrets.service" ];
    serviceConfig.Type = "oneshot";
    script = ''
      ${pkgs.python3}/bin/python3 ${./beszel-setup.py} \
        ${config.sops.secrets.beszel-superuser.path} \
        ${config.sops.secrets.nextcloud-resend.path}
    '';
  };
}
