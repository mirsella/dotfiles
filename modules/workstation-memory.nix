{
  # Logical capacity, not a reservation of physical RAM.
  zramSwap = {
    enable = true;
    algorithm = "zstd";
    memoryPercent = 200;
    priority = 100;
  };

  boot.kernel.sysctl = {
    "vm.swappiness" = 150;
    "vm.watermark_scale_factor" = 125;
    "vm.page-cluster" = 0;
    "vm.watermark_boost_factor" = 0;
  };
}
