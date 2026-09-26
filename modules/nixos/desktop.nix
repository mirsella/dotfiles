{ config, lib, pkgs, ... }:
{
  # Power Profiles Daemon manages CPU energy performance preference (EPP)
  # and provides DBus interface for KDE Plasma / PowerDevil.
  services.power-profiles-daemon.enable = lib.mkDefault true;

  # CoolerControl daemon for managing fan curves
  programs.coolercontrol.enable = lib.mkDefault true;
}
