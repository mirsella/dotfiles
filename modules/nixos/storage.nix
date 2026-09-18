{
  pkgs, ... }:
{
  networking.hostId = "007f0200";
  boot.supportedFilesystems = [ "zfs" ];
  boot.zfs.forceImportRoot = false;
  boot.zfs.extraPools = [ "fast" "tank" ];

  fileSystems."/var/lib/nextcloud/data" = {
    device = "fast/ncdata";
    fsType = "zfs";
    options = [ "nofail" ];
  };

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

  services.sanoid = {
    enable = true;
    datasets = {
      "tank/library" = {
        daily = 7;
        weekly = 4;
        autosnap = true;
        autoprune = true;
        recursive = true;
      };
      "fast/ncdata" = {
        daily = 7;
        weekly = 4;
        autosnap = true;
        autoprune = true;
        recursive = true;
      };
    };
  };
}
