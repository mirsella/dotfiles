{ pkgs, ... }:
{
  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;
  boot.initrd.systemd.enable = true;

  boot.tmp.useTmpfs = true;
  boot.kernelParams = [ "zswap.enabled=0" ];
  zramSwap = {
    enable = true;
    priority = 100;
  };

  services.desktopManager.plasma6.enable = true;
  services.displayManager.sddm = {
    enable = true;
    wayland.enable = true;
  };
  services.power-profiles-daemon.enable = true;
  services.xserver.xkb = {
    layout = "us";
    variant = "colemak_dh_iso";
    model = "pc105";
    options = "terminate:ctrl_alt_bksp";
  };
  console.useXkbConfig = true;
  services.pipewire = {
    enable = true;
    alsa.enable = true;
    alsa.support32Bit = true;
    pulse.enable = true;
  };
  security.rtkit.enable = true;
  hardware.graphics = {
    enable = true;
    enable32Bit = true;
  };
  hardware.bluetooth.enable = true;

  programs.firefox.enable = true;
  programs.gnupg.agent.enable = true;
  environment.systemPackages = [ pkgs.wezterm ];
}
