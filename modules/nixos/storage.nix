{ pkgs, lib, ... }:
{
  imports = [ ../../disko.nix ];

  networking.hostId = "007f0200";
  boot.supportedFilesystems = [ "zfs" ];
  boot.zfs.forceImportRoot = false;

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

  fileSystems = lib.genAttrs [
    "/var/lib/nextcloud"
    "/var/lib/postgresql"
    "/srv/services"
    "/srv/storage"
    "/srv/storage/photos"
    "/srv/storage/archive"
  ] (_: { options = [ "nofail" ]; });

  services.sanoid = {
    enable = true;
    datasets."tank/library" = {
      hourly = 24;
      daily = 7;
      weekly = 4;
      monthly = 3;
      autosnap = true;
      autoprune = true;
      recursive = true;
    };
  };
}
