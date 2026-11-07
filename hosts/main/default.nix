{ lib, pkgs, ... }:
let
  fanUnit =
    file:
    lib.replaceStrings
      [
        "/usr/local/libexec/host-tools"
        "/usr/bin/fancontrol"
        "/etc/main-fan-control.json"
        "/usr/bin:/usr/sbin"
      ]
      [
        "${pkgs.host-tools}/bin/host-tools"
        "${pkgs.lm_sensors}/bin/fancontrol"
        "${./fan-control.json}"
        (lib.makeBinPath [
          pkgs.coreutils
          pkgs.gnugrep
          pkgs.gnused
          pkgs.systemd
        ])
      ]
      (builtins.readFile file);
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

  # NCT6798 fan controller and the AMD SMBus used by the RAM lighting.
  boot.kernelModules = [ "nct6775" "i2c-piix4" ];
  systemd.units."fancontrol.service" = {
    text = fanUnit ./fancontrol.service;
    wantedBy = [ "multi-user.target" ];
  };
  systemd.units."fan-profile-watch.service" = {
    text = fanUnit ./fan-profile-watch.service;
    wantedBy = [ "multi-user.target" ];
  };

  hardware.i2c.enable = true;
  environment.etc."main-rgb/OpenRGB.json".source = ./OpenRGB.json;
  systemd.units."main-rgb.service" = {
    text = lib.replaceStrings
      [
        "/usr/bin/openrgb"
        "/usr/bin/cp"
        "/usr/bin/liquidctl"
      ]
      [
        "${pkgs.openrgb}/bin/openrgb"
        "${pkgs.coreutils}/bin/cp"
        "${pkgs.liquidctl}/bin/liquidctl"
      ]
      (builtins.readFile ./main-rgb.service);
    wantedBy = [
      "graphical.target"
      "suspend.target"
      "hibernate.target"
      "hybrid-sleep.target"
      "suspend-then-hibernate.target"
    ];
  };

  system.stateVersion = "26.05";
}
