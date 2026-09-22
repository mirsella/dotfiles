{ inputs, ... }:
{
  imports = [
    ../modules/home/common.nix
    ../modules/home/workstation.nix
    ../modules/home/llama-server.nix
    inputs.sops-nix.homeManagerModules.sops
  ];
  targets.genericLinux.enable = true;
}
