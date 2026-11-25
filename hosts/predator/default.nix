{ config, lib, pkgs, inputs, ... }:
let
  flakeDir = "/home/mirsella/dev/dotfiles";
in
{
  imports = [
    ./boot.nix
    ./lid-screen.nix
    ./storage-alerts.nix
    ./trash.nix
    ./hermes
    ../../modules/nixos/healthchecks.nix
    ../../modules/nixos/storage.nix
    ../../modules/nixos/caddy.nix
    ../../modules/nixos/nextcloud.nix
    ../../modules/nixos/immich.nix
    ../../modules/nixos/ente.nix
    ../../modules/nixos/backups.nix
    ../../modules/nixos/monitoring.nix
  ];

  predator.hermes.enable = true;
  predator.hermes.messaging = true;

  nixpkgs.overlays = [ inputs.nix-cachyos-kernel.overlays.pinned ];

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
    blacklistedKernelModules = [
      # Leave the unused NVIDIA GPU unbound so runtime power management can turn it off.
      "nouveau"
      # Keep the vendor WMI driver unloaded, as on the running server.
      "acer_wmi"
    ];
    # Reboot after a detected CPU lockup or panic instead of waiting for a person.
    kernel.sysctl = {
      "kernel.hardlockup_panic" = 1;
      "kernel.softlockup_panic" = 1;
      "kernel.panic" = 10;
    };
    # Prefer lower CPU power and heat on this always-on server; turbo stays enabled.
    kernel.sysfs.devices.system.cpu.cpufreq = lib.genAttrs
      (map (id: "policy${toString id}") (lib.range 0 7))
      (_: { energy_performance_preference = "balance_power"; });
  };

  services.udev.extraRules = ''
    # Select the TCO watchdog by identity; watchdog0/1 numbering can change at boot.
    SUBSYSTEM=="watchdog", ATTR{identity}=="iTCO_wdt", SYMLINK+="watchdog-itco"

    # Allow the unused NVIDIA graphics function and its upstream port to reach D3cold.
    ACTION=="add", SUBSYSTEM=="pci", KERNEL=="0000:01:00.0", DRIVER=="", ATTR{vendor}=="0x10de", ATTR{device}=="0x1c20", ATTR{power/control}="auto"
  '';
  systemd.settings.Manager = {
    WatchdogDevice = "/dev/watchdog-itco";
    RuntimeWatchdogSec = "60s";
    RebootWatchdogSec = "2min";
  };

  networking = {
    # NetworkManager.conf uses the numeric enum, not nmcli's "magic" alias.
    networkmanager.connectionConfig."ethernet.wake-on-lan" = 64;
    # Keep the deployed keyfile name and UUID so a live switch cannot leave
    # two NetworkManager profiles for the same NIC. Caddy binds to this IP.
    networkmanager.ensureProfiles.profiles.predator-pm-study-lan = {
      connection = {
        id = "predator-pm-study-lan";
        uuid = "c4e6863b-7f76-4a4d-9a48-9937af0b5a36";
        type = "ethernet";
        interface-name = "enp3s0f1";
        autoconnect = true;
        autoconnect-priority = 100;
      };
      ethernet = {
        mac-address = "98:29:a6:3a:a7:50";
        wake-on-lan = 64;
      };
      ipv4 = {
        method = "manual";
        address1 = "192.168.1.19/24,192.168.1.1";
        dns = "192.168.1.1;";
      };
      ipv6.method = "auto";
    };
    firewall.allowedTCPPorts = [ 80 443 4096 14096 ];
  };

  services = {
    # Idle time and power-saving keys must not interrupt the server.
    logind.settings.Login = {
      IdleAction = "ignore";
      HandleSuspendKey = "ignore";
      HandleHibernateKey = "ignore";
    };
  };

  nix.settings = {
    # The CachyOS kernel and ZFS packages must be present here before the next rebuild.
    # Rebuilt after the 2026-10 server compromise with a new key; never trust the old one.
    extra-substituters = [ "https://cache.lantian.pub/lantian" ];
    extra-trusted-public-keys = [ "lantian:WVGO7vsRMSEN+h7Lp7kv6SAmrkEXgUGbbXm7jMDjBuQ=" ];
    # 7.6GB RAM: an uncapped rioterm build once OOM-wedged the box.
    max-jobs = 1;
    cores = 4;
    trusted-public-keys = [
      "main-copy:ffHmA9AO/DRnU3OsF4z0ZuMGJN5hkTsun7zvGP9Tn0g="
    ];
  };

  # Leave room for resident services; build large closures on another host.
  # Bound swap as well so memory-heavy builds cannot spend hours thrashing.
  systemd.services.nix-daemon.serviceConfig = {
    MemoryMax = "3G";
    MemorySwapMax = "1G";
  };

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
    serviceConfig.ExecStartPre = "${pkgs.util-linux}/bin/runuser -u mirsella -- ${config.nix.package}/bin/nix flake update nixpkgs";
  };
  services.fstrim = {
    enable = true;
    interval = "Sun 11:00";
  };

  # Order secret installation first without stopping user sessions on rotation.
  systemd.services."user@" = {
    overrideStrategy = "asDropin";
    restartIfChanged = false;
    wants = [ "sops-install-secrets.service" ];
    after = [ "sops-install-secrets.service" ];
  };

  system.stateVersion = "26.05";
}
