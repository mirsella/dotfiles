{ pkgs, inputs, lib, ... }:
{
  imports = [
    ./hosts/predator-hardware.nix
    ./acer-wmi-ph31751-module.nix
    ./modules/nixos/storage.nix
    ./modules/nixos/caddy.nix
    ./modules/nixos/nextcloud.nix
    ./modules/nixos/immich.nix
    ./modules/nixos/openchamber.nix
    ./modules/nixos/night-suspend.nix
  ];

  nixpkgs.overlays = [
    inputs.neovim-nightly-overlay.overlays.default
    (import ./overlays.nix inputs)
  ];

  sops.age.sshKeyPaths = [ "/etc/ssh/ssh_host_ed25519_key" ];
  sops.secrets =
    let
      userSecret = path: {
        sopsFile = ./secrets/services.yaml;
        owner = "mirsella";
        inherit path;
      };
    in
    {
      telegram_env = userSecret "/home/mirsella/.config/telegram.env";
      opencode_server = userSecret "/home/mirsella/.config/opencode/server.env";
      openchamber_server = userSecret "/home/mirsella/.config/openchamber/server.env";
      env_secrets = userSecret "/home/mirsella/.config/environment.d/55-secrets.conf";
      stuff_config = userSecret "/home/mirsella/.config/stuff/config.toml";
      context7_accounts = userSecret "/home/mirsella/.config/context7-account-broker/accounts.json";
      gdrive_gcp = userSecret "/home/mirsella/.config/google-drive-mcp/gcp-oauth.keys.json";
      gdrive_tokens = userSecret "/home/mirsella/.config/google-drive-mcp/tokens.json";
    };

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
    loader.systemd-boot.enable = lib.mkForce false;
    loader.systemd-boot.editor = false;
    loader.efi.canTouchEfiVariables = true;
    lanzaboote = {
      enable = true;
      pkiBundle = "/var/lib/sbctl";
    };
    kernelModules = [ "ec_sys" ];
    extraModprobeConfig = "options ec_sys write_support=1";
  };

  networking = {
    hostName = "predator";
    networkmanager.enable = true;
    firewall.allowedTCPPorts = [ 80 443 4096 4097 14096 14097 ];
  };

  time.timeZone = "Europe/Paris";
  i18n.defaultLocale = "en_US.UTF-8";
  console.keyMap = "fr";

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
    # Laptop-as-server: a closed lid or stray sleep key must never suspend it.
    # (AllowSuspend stays enabled so night-suspend can work; the physical
    # triggers above being ignored is what protects the box.)
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
    defaultNetwork.settings.dns_enabled = true;
  };
  systemd.tmpfiles.rules = [
    "L+ /var/run/docker.sock - - - - /run/podman/podman.sock"
  ];

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

  # 8G swapfile on the encrypted NVMe root as OOM breathing room.
  swapDevices = [ { device = "/swapfile"; size = 8 * 1024; } ];

  # Weekly patch cadence: update nixpkgs alone (security fixes, minimal churn),
  # rebuild and switch in place. No auto-reboot: the new kernel waits for a
  # manual reboot, and the laptop is the primary place for `nix flake update`.
  system.autoUpgrade = {
    enable = true;
    flake = "/home/mirsella/dev/nixos#predator";
    flags = [ "--update-input" "nixpkgs" "--max-jobs" "1" "--cores" "2" ];
    dates = "Sun 04:00";
    randomizedDelaySec = "30min";
    allowReboot = false;
  };

  environment.systemPackages = with pkgs; [
    vim
    git
    cryptsetup
    tpm2-tools
    sbctl
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

  # ethtool WOL is volatile across reboots and some drivers drop it on
  # resume, so re-arm magic-packet wake at boot, before sleep, and at
  # shutdown: whatever state came before, the NIC always ends up listening.
  systemd.services.wol-arm = {
    description = "Arm Wake-on-LAN magic-packet wake on enp3s0f1";
    wantedBy = [
      "multi-user.target"
      "sleep.target"
      "shutdown.target"
    ];
    before = [
      "sleep.target"
      "shutdown.target"
    ];
    after = [ "network-pre.target" ];
    serviceConfig.Type = "oneshot";
    path = with pkgs; [ ethtool ];
    script = ''
      ethtool -s enp3s0f1 wol g
    '';
  };

  system.stateVersion = "26.05";
}
