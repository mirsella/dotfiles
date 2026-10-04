{
  ssds = {
    fast = "/dev/disk/by-id/ata-CT240BX500SSD1_2004E3E6DE68"; # Crucial 240 GB
    root = "/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68"; # Hynix 128 GB
  };
  # USB HDDs in tank; names also identify their mappings and /etc/luks keys.
  hdds = {
    tank1 = "/dev/disk/by-id/wwn-0x5000c500aa3cc143"; # Seagate 1 TB
    tank2 = "/dev/disk/by-id/wwn-0x500003961228993f"; # Toshiba 750 GB
    tank3 = "/dev/disk/by-id/wwn-0x50014ee26a2aa7c6"; # WD 1 TB
  };
}
