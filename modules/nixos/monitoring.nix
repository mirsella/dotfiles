{ config, lib, pkgs, ... }:
{
  # Netdata was dropped: the nixpkgs build ships the API only, no dashboard UI.
  # Beszel covers the disk/analytics need.

  # Beszel hub + local agent. ZFS pools (fast/tank) are picked up
  # automatically; the agent needs the ZFS tools and /dev/zfs access.
  services.beszel.hub = {
    enable = true;
    # Served from a subpath on the main domain (see caddy.nix), so no
    # dedicated vhost or firewall port is needed.
    environment.APP_URL = "https://mirsella.mooo.com/beszel";
    # Heartbeat ping URL for Healthchecks.io, from secrets/beszel.yaml.
    environmentFile = config.sops.secrets.beszel-heartbeat.path;
  };
  services.beszel.agent = {
    enable = true;
    environment.LISTEN = "127.0.0.1:45876";
    extraPath = [ config.boot.zfs.package ];
    # The agent only reports the root disk by default. Fast and Archive
    # are ZFS datasets, so their usage comes from `zfs list`.
    environment.EXTRA_FILESYSTEMS = "/srv/data/fast__Fast-SSD,/srv/data/archive__Archive-HDD,/var/lib/nextcloud/data__Nextcloud";
    # Hub's public key (generated into its state dir on first boot).
    # Public by design: it only authorizes the hub to query this agent.
    environment.KEY = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIO6dTO3k45VaN7Xjt4cKpNF2Fw5TvZEoMW+pmgTjqk6A";
  };
  # The module sandboxes the agent with PrivateDevices, which hides
  # /dev/zfs and breaks ZFS pool monitoring. Relax minimally.
  systemd.services.beszel-agent.serviceConfig = {
    PrivateDevices = lib.mkForce false;
    DeviceAllow = [ "/dev/zfs rw" ];
    # /var/lib/nextcloud is 0750 nextcloud:nextcloud; the agent needs the
    # group to stat the Nextcloud datadir entry.
    SupplementaryGroups = [ "nextcloud" ];
  };

  # Hub credentials and heartbeat live in sops; the Resend key is shared
  # with the Nextcloud mail setup (same sender domain).
  sops.secrets.beszel-heartbeat = {
    sopsFile = ../../secrets/beszel.yaml;
    key = "heartbeat_env";
    restartUnits = [ "beszel-hub.service" ];
  };
  sops.secrets.beszel-superuser = {
    sopsFile = ../../secrets/beszel.yaml;
    key = "superuser_password";
  };

  # Converge PocketBase settings that have no NixOS options: Resend SMTP
  # + sender, the hub admin account, and the local system entry.
  systemd.services.beszel-setup = {
    description = "Converge Beszel hub settings, account and system";
    wantedBy = [ "multi-user.target" ];
    after = [ "beszel-hub.service" "sops-install-secrets.service" ];
    requires = [ "sops-install-secrets.service" ];
    serviceConfig.Type = "oneshot";
    script = ''
      ${pkgs.python3}/bin/python3 ${./beszel-setup.py} \
        ${config.sops.secrets.beszel-superuser.path} \
        ${config.sops.secrets.nextcloud-resend.path}
    '';
  };

}
