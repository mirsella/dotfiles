{ config, pkgs, inputs, ... }:
let
  flakeDir = "/home/mirsella/dev/dotfiles";
in
{
  imports = [
    ./boot.nix
    ./acer-wmi.nix
    ../../modules/nixos/storage.nix
    ../../modules/nixos/caddy.nix
    ../../modules/nixos/nextcloud.nix
    ../../modules/nixos/immich.nix
    ../../modules/nixos/db-backup.nix
    ../../modules/nixos/night-suspend.nix
    ../../modules/nixos/monitoring.nix
  ];

  nixpkgs.overlays = [ inputs.nix-cachyos-kernel.overlays.pinned ];

  boot.kernelPackages = pkgs.cachyosKernels.linuxPackages-cachyos-lts;
  boot.zfs.package = config.boot.kernelPackages.zfs_cachyos;

  sops.age.sshKeyPaths = [ "/etc/ssh/ssh_host_ed25519_key" ];
  sops.useSystemdActivation = true;
  sops.secrets = builtins.mapAttrs (_: path: {
    sopsFile = ../../secrets/services.yaml;
    owner = "mirsella";
    path = "${config.users.users.mirsella.home}/.config/${path}";
  }) (import ../../modules/user-secrets.nix);

  home-manager.users.mirsella.home.file.".local/share/cargo/config.toml".text = ''
    [build]
    jobs = 4
  '';

  boot = {
    kernelModules = [ "ec_sys" ];
    extraModprobeConfig = "options ec_sys write_support=1";
    kernelParams = [ "consoleblank=60" ];
  };

  networking = {
    # NetworkManager.conf uses the numeric enum, not nmcli's "magic" alias.
    networkmanager.connectionConfig."ethernet.wake-on-lan" = 64;
    firewall.allowedTCPPorts = [ 80 443 4096 4097 14096 14097 ];
  };

  services = {
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

  nix.settings = {
    extra-substituters = [ "https://attic.xuyh0120.win/lantian" ];
    extra-trusted-public-keys = [ "lantian:EeAUQ+W+6r7EtwnmYjeVwx5kOGEBpjlBfPlzGlTNvHc=" ];
    # 7.6GB RAM: an uncapped rioterm build once OOM-wedged the box.
    max-jobs = 1;
    cores = 4;
    trusted-public-keys = [
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
    flake = "path:${flakeDir}#${config.networking.hostName}";
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
