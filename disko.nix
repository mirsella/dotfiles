# Disk layout record for disko (flake input). Declares pools, datasets and the
# fileSystems mounts NixOS manages (legacy mountpoints, so systemd orders
# dependents correctly). One-time provisioning was done by hand pool-by-pool
# because sdc staging temporarily lived on the SSD; the values below are what
# was actually created. A normal rebuild never formats anything.
#
# Stable paths (verified 2026-09-18, both HDDs sit on USB bridges):
#   ssd  ata-CT240BX500SSD1_2004E3E6DE68            (Crucial 240GB, sda)
#   hdd1 wwn-0x5000c500aa3cc143  ST1000LM035 1TB    (sdc)
#   hdd2 wwn-0x500003961228993f TOSHIBA 750GB       (sdd)
{
  disko.devices = {
    disk = {
      ssd = {
        type = "disk";
        device = "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68";
        content = {
          type = "gpt";
          partitions.zfs = {
            size = "100%";
            content = {
              type = "zfs";
              pool = "fast";
            };
          };
        };
      };
      hdd1 = {
        type = "disk";
        device = "/dev/disk/by-id/wwn-0x5000c500aa3cc143";
        content = {
          type = "gpt";
          partitions.zfs = {
            size = "100%";
            content = {
              type = "zfs";
              pool = "tank";
            };
          };
        };
      };
      hdd2 = {
        type = "disk";
        device = "/dev/disk/by-id/wwn-0x500003961228993f";
        content = {
          type = "gpt";
          partitions.zfs = {
            size = "100%";
            content = {
              type = "zfs";
              pool = "tank";
            };
          };
        };
      };
    };
    zpool = {
      fast = {
        type = "zpool";
        options = {
          ashift = "12";
          autotrim = "off"; # periodic trim via services.zfs.trim instead
        };
        rootFsOptions = {
          compression = "lz4";
          atime = "off";
          xattr = "sa";
          acltype = "posixacl";
          mountpoint = "none";
        };
        datasets = {
          nextcloud = {
            type = "zfs_fs";
            mountpoint = "/var/lib/nextcloud";
          };
          postgresql = {
            type = "zfs_fs";
            mountpoint = "/var/lib/postgresql";
            options.recordsize = "32K";
          };
          services = {
            type = "zfs_fs";
            mountpoint = "/srv/services";
          };
        };
      };
      tank = {
        type = "zpool";
        mode = "mirror";
        options = {
          ashift = "12";
        };
        rootFsOptions = {
          compression = "lz4";
          atime = "off";
          xattr = "sa";
          acltype = "posixacl";
          mountpoint = "none";
        };
        datasets = {
          library = {
            type = "zfs_fs";
            mountpoint = "/srv/storage";
          };
          "library/photos" = {
            type = "zfs_fs";
            mountpoint = "/srv/storage/photos";
          };
          "library/archive" = {
            type = "zfs_fs";
            mountpoint = "/srv/storage/archive";
          };
          backup = {
            type = "zfs_fs"; # inherits mountpoint=none: never mounted
          };
          "backup/fast" = {
            type = "zfs_fs"; # inherits mountpoint=none: replication target only
          };
        };
      };
    };
  };
}
