# Standalone fresh-install formatter for all four data disks, excluding root/ESP.
# Installed targets read its mount layout, never its formatter. Recover existing
# disks by mounting them; formatting destroys data and enrolled TPM/HDD keys.
{
  disko.devices =
    let
      physical = import ./hosts/predator/disks.nix;
      luksZfsDisk = pool: allowDiscards: name: device: {
        type = "disk";
        inherit device;
        content = {
          type = "gpt";
          partitions.crypt = {
            size = "100%";
            content = {
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
          };
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
        ssd = luksZfsDisk "fast" true "fast" physical.ssds.fast;
      } // builtins.mapAttrs (luksZfsDisk "tank" false) physical.hdds;
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
            backup = zfsFs "/srv/backup";
            "backup/recovery" = zfsFs "/srv/backup/recovery";
            replica = {
              type = "zfs_fs";
              options = { mountpoint = "none"; canmount = "off"; readonly = "on"; };
            };
          };
        };
      };
    };
}
