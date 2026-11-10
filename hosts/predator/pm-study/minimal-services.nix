# Optional application-stack isolation arm. Boot it explicitly; the normal host
# configuration does not import this module. Keep storage, drivers and PM fixed.
{ lib, ... }:
let
  services = [
    "beszel-agent" "beszel-hub" "beszel-setup" "caddy" "nginx"
    "nextcloud-setup" "nextcloud-update-db" "nextcloud-cron" "phpfpm-nextcloud"
    "postgresql" "postgresql-setup" "redis-nextcloud"
    "podman-immich_server" "podman-immich_ml" "podman-immich_postgres"
    "podman-immich_redis" "podman" "podman-auto-update" "podman-restart"
    "hermes-agent" "hermes-backend" "hermes-browser-control"
    "hermes-secret-service" "camofox-browser" "sleev-gateway"
    "home-manager-mirsella"
    "db-backup" "hermes-backup" "recovery-backup" "restic-backups-fast"
    "restic-check" "sanoid" "zfs-scrub" "zpool-trim" "fstrim"
    "nix-gc" "nixos-upgrade" "fwupd" "fwupd-refresh"
  ];
  timers = [
    "nextcloud-cron" "db-backup" "hermes-backup" "recovery-backup"
    "restic-backups-fast" "restic-check" "sanoid" "zfs-scrub" "zpool-trim"
    "fstrim" "nix-gc" "nixos-upgrade" "fwupd-refresh" "podman-auto-update"
  ];
  # A false assertion would fail the boot job. A false condition skips the
  # original unit, including socket/D-Bus/dependency activation, without running
  # its setup or executable. Retain the original definitions and packages.
  skip = {
    unitConfig.ConditionPathExists = lib.mkForce "/run/predator-pm-applications-enabled";
  };
in
{
  system.nixos.tags = [ "pm-applications-off" ];
  systemd.services = lib.genAttrs services (_: skip) // {
    # Includes lingering Home Manager agents. SSH and its PAM runtime directory
    # remain available; the user service manager itself is deliberately skipped.
    "user@" = skip;
  };
  systemd.timers = lib.genAttrs timers (_: { enable = lib.mkForce false; });
  systemd.sockets.podman.enable = lib.mkForce false;
  environment.etc."predator-pm-application-isolation.json".text = builtins.toJSON {
    schema = 1;
    inherit services timers;
    skippedUserManager = "user@1000.service";
    absentConditionPath = "/run/predator-pm-applications-enabled";
    purpose = "Application-stack screening, not a driver-minimal or storage-free boot";
  };
}
