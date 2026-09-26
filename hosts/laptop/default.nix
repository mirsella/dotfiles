{ lib, pkgs, ... }:
let
  ryzenadjWatch = pkgs.writeShellApplication {
    name = "ryzenadj-laptop";
    runtimeInputs = with pkgs; [ ryzenadj coreutils gnugrep gawk ];
    text = lib.removePrefix "#!/bin/sh\n"
      (builtins.readFile ../../dotfiles/dot_local/bin/executable_ryzenadj-laptop);
  };
in
{
  imports = [ ../../modules/nixos/desktop.nix ];

  boot.initrd.luks.devices.root.crypttabExtraOpts = [ "tpm2-device=auto" ];
  fileSystems."/".options = [ "compress=zstd:3" ];
  swapDevices = [ { device = "/swap/swapfile"; } ];
  boot.tmp.tmpfsSize = "16G";

  hardware.cpu.amd.ryzen-smu.enable = true;
  systemd.services.ryzenadj-laptop = {
    description = "Apply RyzenAdj performance limits";
    wantedBy = [ "multi-user.target" ];
    after = [ "power-profiles-daemon.service" ];
    serviceConfig = {
      ExecStart = "${lib.getExe ryzenadjWatch} --watch";
      Restart = "on-failure";
      RestartSec = 5;
    };
  };

  system.stateVersion = "26.05";
}
