# NixOS configuration for Predator (Acer Predator, i7-7700HQ)
# Target: 192.168.1.19 live installer -> /dev/sdb (128GB Hynix)
# Installed via SSH from mirsella laptop
{ config, pkgs, ... }:
{
  imports = [ ./hardware-configuration.nix ./acer-wmi-ph31751-module.nix ./modules/nixos/storage.nix ];

  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;
  boot.loader.efi.efiSysMountPoint = "/boot";

  networking.hostName = "predator";
  networking.networkmanager.enable = true;

  time.timeZone = "Europe/Paris";
  i18n.defaultLocale = "en_US.UTF-8";
  console.keyMap = "fr";

  services.openssh = {
    enable = true;
    settings = {
      # Password login allowed: strong passphrase + fail2ban (see below).
      PasswordAuthentication = true;
      KbdInteractiveAuthentication = true;
      PermitRootLogin = "no";
    };
    openFirewall = true;
  };

  services.fail2ban = {
    enable = true;
    maxretry = 5;
    bantime = "1h";
    ignoreIP = [ "127.0.0.1/8" "192.168.1.0/24" ];
    jails.sshd.settings.enabled = true;
  };

  networking.firewall.allowedTCPPorts = [ 22 80 443 4096 4097 ];

  users.users.mirsella = {
    isNormalUser = true;
    description = "mirsella";
    extraGroups = [ "wheel" "networkmanager" ];
    shell = pkgs.bash;
    hashedPassword = "$6$u95TjqJ3iGNjVYkK$uZJx66pXAgvJmSXtNR7oY4dAOUSMyDJVm5CxxDzQnRCu1YTf1bJAkEoKn3VzqdyTWyt7MOBdRxY8DHGdZdLZZ0";
    openssh.authorizedKeys.keys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGkl0CiN6/cLz1OOzBvHaPAMKTnYI0sOlKFDRW25uReF"
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMXuc6N//8+RfjUhuRZ+COgynjfwFqAeoKAWMUz6s+Pe mirsella@main"
    ];
  };

  security.sudo.wheelNeedsPassword = false;
  nixpkgs.config.allowUnfree = true;
  hardware.enableRedistributableFirmware = true;

  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  environment.systemPackages = with pkgs; [
    vim
    git
    curl
    wget
    htop
    efibootmgr
  ];

  # Predator PH317-51 keyboard backlight: EC-owned, no sysfs/WMI knob.
  # Reverse-engineered 2026-09-18: EC RAM bytes 0x30/0x31 = 01 when lit,
  # 00 when off (Fn+F9 flips them). Zero them at boot so it stays off.
  boot.kernelModules = [ "ec_sys" ];
  boot.extraModprobeConfig = ''
    options ec_sys write_support=1
  '';

  systemd.services.predator-kbd-backlight-off = {
    description = "Turn off Predator keyboard backlight (EC 0x30/0x31)";
    wantedBy = [ "multi-user.target" ];
    after = [ "systemd-modules-load.service" ];
    serviceConfig.Type = "oneshot";
    path = with pkgs; [ coreutils ];
    script = ''
      for off in 48 49; do
        printf '\x00' | dd of=/sys/kernel/debug/ec/ec0/io bs=1 seek=$off count=1 conv=notrunc status=none
      done
    '';
  };

  system.stateVersion = "26.05";
}
