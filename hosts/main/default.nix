{ pkgs, ... }:
let
  coolerConfigDir = ../../dotfiles/system/main/etc/coolercontrol;
in
{
  imports = [ ../../modules/nixos/desktop.nix ];

  # The scanner omits mounts under /run and existing swap files.
  fileSystems."/run/media/mirsella/data" = {
    device = "/dev/disk/by-uuid/58D9ECF16BCE7A8E";
    fsType = "ntfs-3g";
    options = [
      "rw"
      "relatime"
    ];
  };
  boot.tmp.tmpfsSize = "32G";

  # NCT6798 motherboard fan controller.
  boot.kernelModules = [ "nct6775" ];
  programs.coolercontrol.enable = true;
  systemd.services.coolercontrold.serviceConfig = {
    ConfigurationDirectory = "coolercontrol";
    # The daemon writes these files. NixOS stops the unit before ExecStartPre
    # and starts it again afterwards by default, which is also the invariant
    # tests/hosts.nix holds in place.
    ExecStartPre = "${pkgs.coreutils}/bin/install -m 0644 -t /etc/coolercontrol ${coolerConfigDir}/config.toml ${coolerConfigDir}/modes.json";
  };

  system.stateVersion = "26.05";
}
