# ZFS storage: fast SSD pool + tank HDD mirror. Disks provisioned once
# (see disko.nix, the layout record); a normal rebuild never touches them.
{ pkgs, lib, ... }:
{
  imports = [ ../../disko.nix ];

  networking.hostId = "007f0200"; # stable id required for ZFS
  boot.supportedFilesystems = [ "zfs" ];
  boot.zfs.extraPools = [ "fast" "tank" ];

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

  # Mounts resolve once the pools exist; never block boot before that.
  fileSystems = lib.genAttrs [
    "/var/lib/nextcloud"
    "/var/lib/postgresql"
    "/srv/services"
    "/srv/storage"
    "/srv/storage/photos"
    "/srv/storage/archive"
  ] (_: { options = [ "nofail" ]; });

  # tank snapshots; fast datasets + syncoid replication added once fast exists.
  services.sanoid = {
    enable = true;
    datasets = {
      "tank/library" = {
        hourly = 24;
        daily = 7;
        weekly = 4;
        monthly = 3;
        autosnap = true;
        autoprune = true;
        recursive = true;
      };
    };
  };
}
