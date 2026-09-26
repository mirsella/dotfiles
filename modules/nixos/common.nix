{ config, lib, pkgs, ... }:
{
  nixpkgs.hostPlatform = lib.mkDefault "x86_64-linux";
  nixpkgs.config.allowUnfree = true;
  nix.settings.experimental-features = [ "nix-command" "flakes" ];
  hardware.enableRedistributableFirmware = true;

  networking.networkmanager.enable = true;
  time.timeZone = "Europe/Paris";
  i18n.defaultLocale = "en_US.UTF-8";

  users.users.mirsella = {
    isNormalUser = true;
    linger = true;
    shell = pkgs.nushell;
    extraGroups = [ "wheel" "networkmanager" "podman" ];
    hashedPassword = "$6$u95TjqJ3iGNjVYkK$uZJx66pXAgvJmSXtNR7oY4dAOUSMyDJVm5CxxDzQnRCu1YTf1bJAkEoKn3VzqdyTWyt7MOBdRxY8DHGdZdLZZ0";
    openssh.authorizedKeys.keys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGkl0CiN6/cLz1OOzBvHaPAMKTnYI0sOlKFDRW25uReF"
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMXuc6N//8+RfjUhuRZ+COgynjfwFqAeoKAWMUz6s+Pe mirsella@main"
    ];
  };
  security.sudo.wheelNeedsPassword = false;

  services = {
    openssh = {
      enable = true;
      openFirewall = true;
      settings = {
        PasswordAuthentication = true;
        KbdInteractiveAuthentication = true;
        PermitRootLogin = "no";
      };
    };
    envfs.enable = true;
    fwupd.enable = true;
  };

  virtualisation.podman = {
    enable = true;
    dockerCompat = true;
    dockerSocket.enable = true;
    defaultNetwork.settings.dns_enabled = true;
  };

  home-manager = {
    useGlobalPkgs = true;
    useUserPackages = true;
    extraSpecialArgs = {
      isNixOS = true;
      hostName = config.networking.hostName;
    };
    users.mirsella.imports = [
      ../home/common.nix
      ../home/nixos.nix
    ];
  };

  environment.systemPackages = with pkgs; [
    vim git ffmpeg imagemagick ntfs3g curl wget htop lm_sensors efibootmgr sops
  ];
}
