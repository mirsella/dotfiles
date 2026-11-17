rec {
  ssds = {
    fast = "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68"; # Crucial 240 GB
    root = "/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68"; # Hynix 128 GB
  };
  # Main partitions form tank; the three larger HDDs also hold backup partitions.
  hdds = {
    tank1 = "/dev/disk/by-id/wwn-0x5000c500aa3cc143"; # Seagate 1 TB
    tank2 = "/dev/disk/by-id/wwn-0x500003961228993f"; # Toshiba 750 GB
    tank3 = "/dev/disk/by-id/wwn-0x50014ee26a2aa7c6"; # WD 1 TB
    tank4 = "/dev/disk/by-id/wwn-0x50014ee6b27a59de"; # WD 1 TB, WXK1A67NTCDX
  };
  # MiB, rounded down from 750 GB; fits the Toshiba with room for GPT.
  mainPartitionSize = "715255M";
  backupHdds = {
    backup1 = hdds.tank1;
    backup3 = hdds.tank3;
    backup4 = hdds.tank4;
  };
  luks = {
    root = "${ssds.root}-part2";
    fast = "${ssds.fast}-part1";
  }
  // builtins.mapAttrs (_: device: "${device}-part1") hdds
  // builtins.mapAttrs (_: device: "${device}-part2") backupHdds;
}
