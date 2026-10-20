{ lib, pkgs, ... }:
{
  imports = [ ../../modules/nixos/desktop.nix ];

  boot.tmp.tmpfsSize = "16G";

  services.fprintd.enable = true;
  environment.etc."xdg/kscreenlockerrc".text = ''
    [Authenticators]
    Fingerprint=true
  '';

  hardware.cpu.amd.ryzen-smu.enable = true;
  systemd.services.ryzenadj-laptop = {
    description = "Maintain RyzenAdj limits and recover stuck performance throttling";
    wantedBy = [ "multi-user.target" ];
    after = [ "power-profiles-daemon.service" ];
    path = [ pkgs.ryzenadj pkgs.power-profiles-daemon ];
    serviceConfig = {
      ExecStart = "${lib.getExe pkgs.host-tools} ryzenadj --watch";
      Restart = "on-failure";
      RestartSec = 5;
      StateDirectory = "ryzenadj-laptop";
      StateDirectoryMode = "0700";
    };
  };

  system.stateVersion = "26.05";
}
