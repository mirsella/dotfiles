{ config, lib, ... }:
let
  mainCoolerConfig = ../../dotfiles/system/main/etc/coolercontrol/config.toml;
in
{
  services.power-profiles-daemon.enable = lib.mkDefault true;
  programs.coolercontrol.enable = lib.mkDefault true;

  systemd.tmpfiles.rules = lib.mkIf (config.networking.hostName == "main") [
    "C /etc/coolercontrol/config.toml 0644 root root - ${mainCoolerConfig}"
  ];
}
