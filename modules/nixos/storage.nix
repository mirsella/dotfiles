{ lib, pkgs, ... }:
{
  networking.hostId = "007f0200";
  boot.supportedFilesystems = [ "zfs" ];
  boot.zfs.forceImportRoot = false;
  boot.zfs.extraPools = [ "fast" ];

  fileSystems = lib.genAttrs [
    "/var/lib/nextcloud"
    "/var/lib/postgresql"
    "/srv/services"
  ] (_: { options = [ "nofail" ]; });

  services.zfs = {
    autoScrub = {
      enable = true;
      interval = "monthly";
      pools = [ "fast" ];
    };
    trim = {
      enable = true;
      interval = "weekly";
    };
  };

  environment.systemPackages = with pkgs; [ smartmontools ];
}
