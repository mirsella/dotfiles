{ config, pkgs, inputs, overlays, ... }:
let
  flakeDir = "/home/mirsella/dev/dotfiles";
in
{
  imports = [
    ./hosts/predator-boot.nix
    ./acer-wmi-ph31751-module.nix
    ./modules/nixos/storage.nix
    ./modules/nixos/caddy.nix
    ./modules/nixos/nextcloud.nix
    ./modules/nixos/immich.nix
    ./modules/nixos/db-backup.nix
    ./modules/nixos/openchamber.nix
    ./modules/nixos/night-suspend.nix
    ./modules/nixos/monitoring.nix
  ];

  nixpkgs.overlays = [ inputs.nix-cachyos-kernel.overlays.pinned ] ++ overlays;

  boot.kernelPackages = pkgs.cachyosKernels.linuxPackages-cachyos-lts;
  boot.zfs.package = config.boot.kernelPackages.zfs_cachyos;

  sops.age.sshKeyPaths = [ "/etc/ssh/ssh_host_ed25519_key" ];
  sops.useSystemdActivation = true;
  sops.secrets = builtins.mapAttrs (_: path: {
    sopsFile = ./secrets/services.yaml;
    owner = "mirsella";
    path = "${config.users.users.mirsella.home}/.config/${path}";
  }) (import ./modules/user-secrets.nix);

  home-manager = {
    useGlobalPkgs = true;
    useUserPackages = true;
    extraSpecialArgs = {
      gitSigningKey = null;
      isNixOS = true;
    };
    users.mirsella = {
      imports = [
        ./modules/home/common.nix
        ./modules/home/server.nix
      ];
    };
  };

  boot = {
    kernelModules = [ "ec_sys" ];
    extraModprobeConfig = "options ec_sys write_support=1";
    kernelParams = [ "consoleblank=60" ];
  };

  networking = {
    hostName = "predator";
    networkmanager.enable = true;
    # NetworkManager.conf uses the numeric enum, not nmcli's "magic" alias.
    networkmanager.connectionConfig."ethernet.wake-on-lan" = 64;
    firewall.allowedTCPPorts = [ 80 443 4096 4097 14096 14097 ];
  };

  time.timeZone = "Europe/Paris";
  i18n.defaultLocale = "en_US.UTF-8";

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
      maxretry = 5;
      bantime = "1h";
      ignoreIP = [ "127.0.0.1/8" "192.168.1.0/24" ];
      jails.sshd.settings.enabled = true;
    };
    # Only the midnight timer may suspend the server, not the lid.
    logind.settings.Login = {
      HandleLidSwitch = "ignore";
      HandleLidSwitchExternalPower = "ignore";
      HandleLidSwitchDocked = "ignore";
      HandleSuspendKey = "ignore";
      HandleHibernateKey = "ignore";
    };
  };

  users.users.mirsella = {
    isNormalUser = true;
    linger = true;
    shell = pkgs.nushell;
    extraGroups = [
      "wheel"
      "networkmanager"
      "podman"
    ];
    hashedPassword = "$6$u95TjqJ3iGNjVYkK$uZJx66pXAgvJmSXtNR7oY4dAOUSMyDJVm5CxxDzQnRCu1YTf1bJAkEoKn3VzqdyTWyt7MOBdRxY8DHGdZdLZZ0";
    openssh.authorizedKeys.keys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGkl0CiN6/cLz1OOzBvHaPAMKTnYI0sOlKFDRW25uReF"
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMXuc6N//8+RfjUhuRZ+COgynjfwFqAeoKAWMUz6s+Pe mirsella@main"
    ];
  };
  security.sudo.wheelNeedsPassword = false;
  services.envfs.enable = true;
  services.fwupd.enable = true;

  virtualisation.podman = {
    enable = true;
    dockerCompat = true;
    dockerSocket.enable = true;
    defaultNetwork.settings.dns_enabled = true;
  };

  nixpkgs.config.allowUnfree = true;
  hardware.enableRedistributableFirmware = true;
  nix.settings = {
    experimental-features = [ "nix-command" "flakes" ];
    # 7.6GB RAM: an uncapped rioterm build once OOM-wedged the box.
    max-jobs = 1;
    cores = 4;
    trusted-public-keys = [
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
      "main-copy:ffHmA9AO/DRnU3OsF4z0ZuMGJN5hkTsun7zvGP9Tn0g="
    ];
  };

  # Builds run on this box: cap the daemon so a hungry compile fails instead of wedging the system.
  systemd.services.nix-daemon.serviceConfig.MemoryMax = "6G";

  # Swap remains inside the encrypted root filesystem.
  swapDevices = [ { device = "/swapfile"; size = 8 * 1024; } ];

  # Update nixpkgs weekly; update the kernel input separately after checking ZFS compatibility.
  system.autoUpgrade = {
    enable = true;
    flake = "path:${flakeDir}#predator";
    upgrade = false;
    flags = [ "--no-update-lock-file" ];
    dates = "Sun 10:00";
    randomizedDelaySec = "30min";
    allowReboot = false;
  };
  systemd.services.nixos-upgrade = {
    serviceConfig.WorkingDirectory = flakeDir;
    preStart = ''
      ${pkgs.util-linux}/bin/runuser -u mirsella -- ${config.nix.package}/bin/nix flake update nixpkgs
    '';
  };
  services.fstrim.interval = "Sun 11:00";

  # Order secret installation first without stopping user sessions on rotation.
  systemd.services."user@" = {
    overrideStrategy = "asDropin";
    restartIfChanged = false;
    wants = [ "sops-install-secrets.service" ];
    after = [ "sops-install-secrets.service" ];
  };

  environment.systemPackages = with pkgs; [
    vim
    git
    ffmpeg
    imagemagick
    ntfs3g
    curl
    wget
    htop
    lm_sensors
    efibootmgr
    sops
  ];

  # EC-owned keyboard backlight, no sysfs knob: EC RAM 0x30/0x31 read 01 when
  # lit (Fn+F9 flips them). Zero them at boot so it stays off.
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
