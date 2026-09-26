{ config, lib, pkgs, ... }:
let
  mainCoolerConfig = ../../dotfiles/system/main/etc/coolercontrol/config.toml;
in
{
  # Power Profiles Daemon manages CPU energy performance preference (EPP)
  # and provides DBus interface for KDE Plasma / PowerDevil.
  services.power-profiles-daemon.enable = lib.mkDefault true;

  # CoolerControl daemon for managing fan curves
  programs.coolercontrol.enable = lib.mkDefault true;

  # When the host is 'main', automatically install the tracked 3-level fan curve
  # into /etc/coolercontrol/config.toml on boot or activation if not present.
  systemd.tmpfiles.rules = lib.mkIf (config.networking.hostName == "main") [
    "C /etc/coolercontrol/config.toml 0644 root root - ${mainCoolerConfig}"
  ];
}
