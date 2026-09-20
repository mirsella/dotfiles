# ssd:  Crucial 240GB, ata-CT240BX500SSD1_2004E3E6DE68 (sda)
# hdd1: Seagate 1TB,   wwn-0x5000c500aa3cc143  (sdc, USB bridge)
# hdd2: Toshiba 750GB, wwn-0x500003961228993f (sdd, USB bridge)
{
  disko.devices =
    let
      zfsDisk = device: pool: {
        type = "disk";
        inherit device;
        content = {
          type = "gpt";
          partitions.zfs = {
            size = "100%";
            content = {
              type = "zfs";
              inherit pool;
            };
          };
        };
      };
      zfsFs = mountpoint: {
        type = "zfs_fs";
        inherit mountpoint;
      };
      commonRootFsOptions = {
        compression = "lz4";
        atime = "off";
        xattr = "sa";
        acltype = "posixacl";
        mountpoint = "none";
      };
    in
    {
      disk = {
        ssd = zfsDisk "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68" "fast";
        hdd1 = zfsDisk "/dev/disk/by-id/wwn-0x5000c500aa3cc143" "tank";
        hdd2 = zfsDisk "/dev/disk/by-id/wwn-0x500003961228993f" "tank";
      };
      zpool = {
        fast = {
          type = "zpool";
          options = {
            ashift = "12";
            autotrim = "off";
          };
          rootFsOptions = commonRootFsOptions;
          datasets = {
            ncdata = {
              type = "zfs_fs";
              mountpoint = "legacy";
            };
            data = zfsFs "/srv/data/fast";
          };
        };
        tank = {
          type = "zpool";
          mode = "mirror";
          options.ashift = "12";
          rootFsOptions = commonRootFsOptions;
          datasets = {
            archive = zfsFs "/srv/data/archive";
            backup = zfsFs "/srv/backup";
          };
        };
      };
    };
}
