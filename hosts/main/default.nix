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

  # NCT6798 motherboard fan controller.
  boot.kernelModules = [ "nct6775" ];
  systemd.units."fancontrol.service" = {
    text = fanUnit ./fancontrol.service;
    wantedBy = [ "multi-user.target" ];
  };
  systemd.units."fan-profile-watch.service" = {
    text = fanUnit ./fan-profile-watch.service;
    wantedBy = [ "multi-user.target" ];
  };

  system.stateVersion = "26.05";
}
