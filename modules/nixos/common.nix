{ config, lib, pkgs, ... }:
{
  nixpkgs.hostPlatform = lib.mkDefault "x86_64-linux";
  nixpkgs.config.allowUnfree = true;
  nix.settings = {
    experimental-features = [ "nix-command" "flakes" ];
    min-free = 10 * 1024 * 1024 * 1024;
    max-free = 20 * 1024 * 1024 * 1024;
    auto-optimise-store = true;
  };
  nix.gc = {
    automatic = true;
    dates = "weekly";
    options = "--delete-older-than 14d";
  };
  services.journald.settings.Journal.SystemMaxUse = "1G";
  hardware.enableRedistributableFirmware = true;

  networking.networkmanager.enable = true;
  time.timeZone = "Europe/Paris";
  i18n.defaultLocale = "en_US.UTF-8";

  # Nix owns accounts: passwd changes revert on rebuild, and the password below comes from
  # pass-cli ("mirsella@predator").
  users.mutableUsers = false;
  users.users.mirsella = {
    isNormalUser = true;
    linger = true;
    shell = pkgs.nushell;
    extraGroups = [ "wheel" "networkmanager" "podman" "libvirtd" ];
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
    fail2ban = {
      enable = true;
      maxretry = 3;
      bantime = "1m";
      # First failure costs a minute; each return doubles from five minutes.
      bantime-increment = {
        enable = true;
        maxtime = "1w";
        formula = "ban.Time * (1 if ban.Count <= 0 else 5 * (1 << (ban.Count - 1)))";
      };
      # The module already trusts loopback; only the LAN needs adding.
      ignoreIP = [ "192.168.1.0/24" ];
      jails = {
        DEFAULT.settings.findtime = "10m";
        sshd.settings.enabled = true;
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
