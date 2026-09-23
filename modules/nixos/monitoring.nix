{ config, lib, pkgs, ... }:
let
  sataDisks = [
    "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68"
    "/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68"
  ];
  usbDisks = [
    "/dev/disk/by-id/wwn-0x5000c500aa3cc143"
    "/dev/disk/by-id/wwn-0x500003961228993f"
  ];
in
{
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
    smartmon.deviceAllow = sataDisks ++ usbDisks;
    environment.SMART_DEVICES = lib.concatStringsSep "," (
      sataDisks ++ map (device: "${device}:sat") usbDisks
    );
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
