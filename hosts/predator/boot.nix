{ lib, pkgs, ... }:
let
  physical = import ./disks.nix;
in
{
  imports = [
    ./recovery-boot.nix
    ./authenticated-boot.nix
  ];

  boot = {
    loader.systemd-boot.enable = lib.mkForce false;
    loader.systemd-boot.editor = false;
    loader.efi.canTouchEfiVariables = true;
    lanzaboote = {
      enable = true;
      pkiBundle = "/var/lib/sbctl";
    };
    initrd.systemd.enable = true;
    initrd.luks.devices.crypt-root = {
      device = lib.mkForce physical.luks.root;
      allowDiscards = true;
    };
    supportedFilesystems = [ "zfs" ];
    zfs.forceImportRoot = false;
  };

  fileSystems."/boot".options = lib.mkForce [
    "fmask=0077"
    "dmask=0077"
  ];
  networking.hostId = "007f0200";
  console.keyMap = "fr";
  environment.systemPackages = with pkgs; [
    cryptsetup
    tpm2-tools
    sbctl
  ];
}
