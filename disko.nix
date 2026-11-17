# Standalone fresh-install formatter for the SSD and four HDDs, excluding root/ESP.
# Installed targets read its mount layout, never its formatter. Recover existing
# disks by mounting them; formatting destroys data and enrolled TPM/HDD keys.
{
  disko.devices =
    let
      physical = import ./hosts/predator/disks.nix;
      luksZfs = pool: allowDiscards: name: {
        type = "luks";
        name = "${name}-crypt";
        initrdUnlock = false;
        askPassword = true;
        extraFormatArgs = [ "--type" "luks2" ];
        settings = {
          inherit allowDiscards;
        };
        content = {
          type = "zfs";
          inherit pool;
        };
      };
      hddDisk = name: device: {
        type = "disk";
        inherit device;
        content = {
          type = "gpt";
          partitions = {
            crypt = {
              priority = 1;
              size = physical.mainPartitionSize;
              type = "CA7D7CCB-63ED-4C53-861C-1742536059CC";
              content = luksZfs "tank" false name;
            };
          } // builtins.listToAttrs (
            map (backupName: {
              name = backupName;
              value = {
                priority = 2;
                size = "100%";
                type = "CA7D7CCB-63ED-4C53-861C-1742536059CC";
                content = luksZfs "backups" false backupName;
              };
            }) (builtins.filter (n: physical.backupHdds.${n} == device)
              (builtins.attrNames physical.backupHdds))
          );
        };
      };
      zfsFs = mountpoint: {
        type = "zfs_fs";
        inherit mountpoint;
        mountOptions = [ "nofail" ];
        options.mountpoint = "legacy";
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
        ssd = {
          type = "disk";
          device = physical.ssds.fast;
          content = {
            type = "gpt";
            partitions.crypt = {
              size = "100%";
              content = luksZfs "fast" true "fast";
            };
          };
        };
      } // builtins.mapAttrs hddDisk physical.hdds;
      zpool = {
        fast = {
          type = "zpool";
          options = {
            ashift = "12";
            autotrim = "off";
          };
          rootFsOptions = commonRootFsOptions;
          datasets = {
            ncdata = zfsFs "/var/lib/nextcloud/data";
            data = zfsFs "/srv/data/fast";
          };
        };
        tank = {
          type = "zpool";
          mode = "raidz";
          options = {
            ashift = "12";
            autoexpand = "on";
            autoreplace = "off";
          };
          rootFsOptions = commonRootFsOptions;
          datasets = {
            archive = zfsFs "/srv/data/archive";
          };
        };
        backups = {
          type = "zpool";
          mode = "raidz";
          options = {
            ashift = "12";
            autoexpand = "on";
            autoreplace = "off";
          };
          rootFsOptions = commonRootFsOptions;
          datasets = {
            data = zfsFs "/srv/backup";
            "data/recovery" = zfsFs "/srv/backup/recovery";
          };
        };
      };
    };
}
