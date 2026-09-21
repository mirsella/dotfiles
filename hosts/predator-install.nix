{ config, pkgs, lib, ... }:

{
  imports = [ ./predator-hardware.nix ];

  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;
  boot.initrd.luks.devices."crypt-root" = {
    device = "/dev/disk/by-id/ata-HFS128G39TND-N210A_EI76N026711106D68-part2";
    keyFile = "/etc/luks/root.key";
    allowDiscards = true;
  };
  boot.initrd.secrets."/etc/luks/root.key" = "/etc/luks/root.key";

  fileSystems."/".device = lib.mkForce "/dev/mapper/crypt-root";

  networking.hostName = "predator";
  networking.networkmanager.enable = true;

  services.openssh = {
    enable = true;
    openFirewall = true;
    settings.PermitRootLogin = "prohibit-password";
  };

  users.users.mirsella = {
    isNormalUser = true;
    extraGroups = [ "wheel" ];
    openssh.authorizedKeys.keys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGkl0CiN6/cLz1OOzBvHaPAMKTnYI0sOlKFDRW25uReF"
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMXuc6N//8+RfjUhuRZ+COgynjfwFqAeoKAWMUz6s+Pe"
    ];
  };
  users.users.root.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGkl0CiN6/cLz1OOzBvHaPAMKTnYI0sOlKFDRW25uReF"
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMXuc6N//8+RfjUhuRZ+COgynjfwFqAeoKAWMUz6s+Pe"
  ];
  security.sudo.wheelNeedsPassword = false;

  nix.settings.experimental-features = [ "nix-command" "flakes" ];
  system.stateVersion = "26.05";
}
