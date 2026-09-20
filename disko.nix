# ssd:  Crucial 240GB, ata-CT240BX500SSD1_2004E3E6DE68
# hdd1: Seagate 1TB,   wwn-0x5000c500aa3cc143  (USB bridge)
# hdd2: Toshiba 750GB, wwn-0x500003961228993f (USB bridge)
#
# All data disks are LUKS2 (aes-xts) with keyfiles in /etc/luks/
# (generated once per machine, never committed). ZFS pools sit on
# the decrypted mappers. Fresh machine: create /etc/luks/*.key
# (head -c 64 /dev/urandom), then disko --mode disko.
{
  disko.devices =
    let
      luksZfsDisk = device: cryptName: pool: allowDiscards: keyFile: {
        type = "disk";
        inherit device;
        content = {
          type = "gpt";
          partitions.crypt = {
            size = "100%";
            content = {
              type = "luks";
              name = cryptName;
              initrdUnlock = true;
              settings = {
                inherit keyFile allowDiscards;
              };
              content = {
                type = "zfs";
                inherit pool;
              };
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
        ssd = luksZfsDisk "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68" "fast-crypt" "fast" true "/etc/luks/fast.key";
        hdd1 = luksZfsDisk "/dev/disk/by-id/wwn-0x5000c500aa3cc143" "tank1-crypt" "tank" false "/etc/luks/tank1.key";
        hdd2 = luksZfsDisk "/dev/disk/by-id/wwn-0x500003961228993f" "tank2-crypt" "tank" false "/etc/luks/tank2.key";
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
