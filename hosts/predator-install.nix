{ config, pkgs, ... }:

{
  # Minimal installed system for restoring state before enabling server services.
  # Mount the disks first; see docs/predator-reinstall.md.
  imports = [ ./predator-boot.nix ];

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
  users.users.root.openssh.authorizedKeys.keys = config.users.users.mirsella.openssh.authorizedKeys.keys;
  security.sudo.wheelNeedsPassword = false;

  environment.systemPackages = [ pkgs.git ];
  nix.settings.experimental-features = [ "nix-command" "flakes" ];
  system.stateVersion = "26.05";
}
