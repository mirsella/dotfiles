{ lib, pkgs, ... }:
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
  systemd.units."fancontrol.service" = {
    text = lib.replaceStrings
      [
        "/usr/local/libexec/host-tools"
        "/etc/main-fan-control.json"
      ]
      [
        "${pkgs.host-tools}/bin/host-tools"
        "${./fan-control.json}"
      ]
      (builtins.readFile ./fancontrol.service);
    wantedBy = [ "multi-user.target" ];
  };

  system.stateVersion = "26.05";
}
