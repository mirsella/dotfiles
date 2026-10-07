{ lib, pkgs, ... }:
{
  imports = [ ../../modules/nixos/desktop.nix ];

  boot.tmp.tmpfsSize = "16G";

  hardware.cpu.amd.ryzen-smu.enable = true;
  systemd.services.ryzenadj-laptop = {
    description = "Apply RyzenAdj performance limits";
    wantedBy = [ "multi-user.target" ];
    after = [ "power-profiles-daemon.service" ];
    path = [ pkgs.ryzenadj ];
    serviceConfig = {
      ExecStart = "${lib.getExe pkgs.host-tools} ryzenadj --watch";
      Restart = "on-failure";
      RestartSec = 5;
    };
  };

  system.stateVersion = "26.05";
}
