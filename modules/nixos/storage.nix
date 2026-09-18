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

  services.smartd = {
    enable = true;
    autodetect = false;
    devices = [
      { device = "/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68"; }
      { device = "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68"; }
      {
        device = "/dev/disk/by-id/wwn-0x5000c500aa3cc143";
        options = "-d sat -n standby,q";
      }
      {
        device = "/dev/disk/by-id/wwn-0x500003961228993f";
        options = "-d sat -n standby,q -i 43200";
      }
    ];
  };

  systemd.services.hd-idle = {
    description = "Spin down idle tank HDDs";
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      Type = "forking";
      ExecStart = "${pkgs.hd-idle}/bin/hd-idle -i 0 -a /dev/disk/by-id/wwn-0x5000c500aa3cc143 -i 2700 -c scsi -a /dev/disk/by-id/wwn-0x500003961228993f -i 2700 -c scsi";
    };
  };

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
