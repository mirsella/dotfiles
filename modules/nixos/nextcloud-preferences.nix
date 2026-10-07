{ config, lib, pkgs, ... }:
let
  mounts = [
    { name = "Fast"; path = "/srv/data/fast"; users = [ "mirsella" "admin" ]; }
    { name = "Archive"; path = "/srv/data/archive"; users = [ "mirsella" "admin" ]; }
    { name = "TankBackup"; path = "/srv/backup"; users = [ "admin" ]; }
  ];
  sharing = pkgs.writers.writeJSON "nextcloud-sharing.json" { apps.core = {
    shareapi_enabled = "yes";
    shareapi_allow_links = "yes";
    shareapi_allow_public_upload = "yes";
    shareapi_default_permissions = "1";
    shareapi_expire_after_n_days = null;
  }; };
  preferences = pkgs.writers.writeJSON "nextcloud-preferences.json" {
    inherit sharing mounts;
  };
in
{
  services.nextcloud.settings.quota_include_external_storage = true;
  systemd.services.nextcloud-setup.path = [ config.services.nextcloud.occ ];

  systemd.services.nextcloud-setup.serviceConfig.ExecStartPost = lib.mkAfter [
    "${pkgs.host-tools}/bin/host-tools nextcloud-setup ${preferences}"
  ];
}
