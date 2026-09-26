{ pkgs, ... }:
let
  coolerConfig = pkgs.writeText "coolercontrol.toml"
    (builtins.readFile ../../dotfiles/system/main/etc/coolercontrol/config.toml);
in
{
  imports = [ ../../modules/nixos/desktop.nix ];

  fileSystems."/".options = [ "compress=zstd:3" ];
  # The scanner omits mounts under /run and existing swap files.
  fileSystems."/run/media/mirsella/data" = {
    device = "/dev/disk/by-uuid/58D9ECF16BCE7A8E";
    fsType = "ntfs-3g";
    options = [ "rw" "relatime" ];
  };
  swapDevices = [ { device = "/swap/swapfile"; discardPolicy = "both"; } ];
  boot.tmp.tmpfsSize = "32G";

  # NCT6798 motherboard fan controller.
  boot.kernelModules = [ "nct6775" ];
  programs.coolercontrol.enable = true;
  systemd.services.coolercontrold.serviceConfig = {
    ConfigurationDirectory = "coolercontrol";
    # The daemon writes this file. Restore it only while the daemon is stopped.
    ExecStartPre = "${pkgs.coreutils}/bin/install -m 0644 ${coolerConfig} /etc/coolercontrol/config.toml";
  };

  system.stateVersion = "26.05";
}
