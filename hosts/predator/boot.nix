{ lib, pkgs, ... }:
{
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
      device = lib.mkForce "/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68-part2";
      # A keyFile with tpm2-device means a sealed blob, not a fallback LUKS key.
      crypttabExtraOpts = [ "tpm2-device=auto" ];
      allowDiscards = true;
    };
    supportedFilesystems = [ "zfs" ];
    zfs.forceImportRoot = false;
    kernelParams = [ "usb-storage.quirks=7825:a2a4:u" ];
  };

  fileSystems."/boot".options = lib.mkForce [ "fmask=0077" "dmask=0077" ];
  networking.hostId = "007f0200";
  console.keyMap = "fr";
  environment.systemPackages = with pkgs; [ cryptsetup tpm2-tools sbctl ];
}
