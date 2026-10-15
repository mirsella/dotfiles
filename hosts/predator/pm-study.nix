# Temporary long-awake S3 comparison: remove this import after the study.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  disks = import ./disks.nix;
  tools = pkgs.predator-pm-study;
in
{
  boot.extraModulePackages = [
    (pkgs.callPackage ../../pkgs/predator-pm-study/module.nix {
      kernel = config.boot.kernelPackages.kernel;
    })
  ];
  boot.blacklistedKernelModules = [
    "acer_wmi"
    "acer_wmi_ph31751"
  ];
  systemd.services.predator-kbd-backlight-off.enable = lib.mkForce false;
  systemd.services.predator-pm-preserve-rtc = {
    description = "Preserve suspend phase RTC evidence before clock synchronization";
    restartIfChanged = false;
    wantedBy = [ "sysinit.target" ];
    after = [
      "local-fs.target"
      "systemd-journald.service"
    ];
    before = [ "systemd-timesyncd.service" ];
    path = [
      pkgs.util-linux
      pkgs.systemd
    ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = "${tools}/bin/predator-pm-study preserve-rtc";
    };
    unitConfig.DefaultDependencies = false;
  };
  # RTC recording can leave a future date after a forced recovery reboot.
  # Calendar timers must wait for NTP instead of retaining future trigger times.
  systemd.additionalUpstreamSystemUnits = [ "systemd-time-wait-sync.service" ];
  systemd.services.systemd-time-wait-sync.wantedBy = [ "sysinit.target" ];
  systemd.services.predator-pm-clock-handoff = {
    description = "Restore NTP after initrd networkd hands the LAN to NetworkManager";
    wantedBy = [ "multi-user.target" ];
    wants = [ "network-online.target" ];
    after = [
      "network-online.target"
      "NetworkManager.service"
      "predator-pm-preserve-rtc.service"
    ];
    path = [
      pkgs.networkmanager
      pkgs.util-linux
      pkgs.systemd
      pkgs.coreutils
    ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      TimeoutStartSec = "2min";
      ExecStart = "${tools}/bin/predator-pm-study clock-handoff";
    };
  };
  systemd.services.predator-pm-capture = {
    description = "Restore Predator suspend diagnostic capture";
    wantedBy = [ "multi-user.target" ];
    wants = [ "network-online.target" ];
    after = [ "network-online.target" ];
    path = [
      pkgs.kmod
      pkgs.util-linux
      pkgs.systemd
    ];
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${tools}/bin/predator-pm-study capture";
      TimeoutStartSec = "45s";
    };
  };
  environment.systemPackages = [
    tools
    pkgs.ethtool
  ];
  # Main's one-shot assistant creates boot-pinned timers after the TPM-free
  # SSD unlock and verified Pi wake setup. Expired calendar tests are retired.
  networking.networkmanager.ensureProfiles.profiles.predator-pm-study-lan = {
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

  specialisation.pm-study-ec-on.configuration = {
    system.nixos.tags = [ "pm-study-ec-on" ];
    systemd.services.predator-kbd-backlight-off = {
      # Override the study's forced disable only in this deliberately selected arm.
      enable = lib.mkOverride 49 true;
      restartIfChanged = false;
      stopIfChanged = false;
      serviceConfig.RemainAfterExit = true;
    };
  };
  specialisation.pm-study.configuration = {
    system.nixos.tags = [ "pm-study" ];
    boot.kernelParams = [ "initcall_blacklist=crb_acpi_driver_init" ];
    boot.initrd.systemd.tpm2.enable = false;
    systemd.tpm2.enable = false;
    boot.initrd.luks.devices.crypt-root.crypttabExtraOpts = lib.mkForce [ ];
    boot.initrd.availableKernelModules = [ "r8169" ];
    boot.initrd.systemd.network = {
      enable = true;
      networks."10-predator-pm-unlock" = {
        matchConfig.MACAddress = "98:29:a6:3a:a7:50";
        address = [ "192.168.1.19/24" ];
        networkConfig = {
          DHCP = "no";
          DNS = [ "192.168.1.1" ];
        };
        routes = [ { Gateway = "192.168.1.1"; } ];
      };
    };
    # The encrypted recovery identity cannot unseal with Linux TPM disabled.
    # Keep this arm console-only; never place a plaintext private key on the ESP.
    boot.initrd.network.ssh.enable = lib.mkForce false;
    boot.initrd.network.ssh.hostKeys = lib.mkForce [ ];
    boot.initrd.systemd.extraBin = {
      predator-pm-study = "${tools}/bin/predator-pm-study";
      systemd-reply-password = "${pkgs.systemd}/lib/systemd/systemd-reply-password";
    };
    environment.etc.crypttab.text = lib.mkForce ''
      fast-crypt ${disks.ssds.fast}-part1 - discard,nofail,headless=true,x-systemd.device-timeout=30s
      tank1-crypt ${disks.hdds.tank1}-part1 - nofail,headless=true,x-systemd.device-timeout=30s
      tank2-crypt ${disks.hdds.tank2}-part1 - nofail,headless=true,x-systemd.device-timeout=30s
      tank3-crypt ${disks.hdds.tank3}-part1 - nofail,headless=true,x-systemd.device-timeout=30s
    '';
  };
}
