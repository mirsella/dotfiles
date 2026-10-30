# Evaluate the installed hardware layouts and host-role boundaries.
flake:
let
  inherit (flake.inputs.nixpkgs) lib;
  hosts = lib.mapAttrs (_: host: host.config) flake.nixosConfigurations;
  desktops = { inherit (hosts) main laptop; };
  server = hosts.predator;
  main = hosts.main;
  laptop = hosts.laptop;
  cooling = main.systemd.units."fancontrol.service";
  watch = main.systemd.units."fan-profile-watch.service";
  workstationHomes = lib.concatMap (name: [
    desktops.${name}.home-manager.users.mirsella
    flake.homeConfigurations.${name}.config
  ]) (builtins.attrNames desktops);
in
assert lib.assertMsg (lib.all (
  home:
  let
    kache = home.home.file.".local/bin/kache".source;
  in
  home.systemd.user.services.kache.Service.ExecStart == [ "${kache} daemon run" ]
) workstationHomes) "Cargo's managed kache wrapper must match the daemon binary";
assert lib.assertMsg (lib.all (host: host.nix.settings.fallback) (builtins.attrValues hosts))
  "Optional binary cache outages must not abort NixOS builds";
assert lib.assertMsg (
  builtins.attrNames flake.nixosModules == [
    "laptop"
    "main"
    "predator"
  ]
  &&
    builtins.attrNames flake.nixosConfigurations == [
      "laptop"
      "main"
      "predator"
    ]
  &&
    builtins.attrNames flake.homeConfigurations == [
      "laptop"
      "main"
    ]
  && lib.all (name: desktops.${name}.networking.hostName == name) [
    "main"
    "laptop"
  ]
) "Host discovery must select the correct hostname and standalone workstation homes";
assert lib.assertMsg (
  !server.services.desktopManager.plasma6.enable
  && !server.services.displayManager.sddm.enable
  && !server.hardware.cpu.amd.ryzen-smu.enable
  && lib.all (
    c:
    c.services.desktopManager.plasma6.enable
    && c.services.displayManager.sddm.enable
    && c.services.pipewire.enable
    && c.services.power-profiles-daemon.enable
    && !c.services.nextcloud.enable
    && !c.services.immich.enable
    && !c.boot.lanzaboote.enable
  ) (builtins.attrValues desktops)
) "Desktop and server roles must remain separate";
assert lib.assertMsg (
  laptop.services.fprintd.enable
  && laptop.security.pam.services.kde-fingerprint.fprintAuth
  && !laptop.security.pam.services.kde.fprintAuth
  && lib.hasInfix "Fingerprint=true" laptop.environment.etc."xdg/kscreenlockerrc".text
  &&
    lib.all (host: !host.services.fprintd.enable && !(host.environment.etc ? "xdg/kscreenlockerrc"))
      [
        main
        server
      ]
) "Only laptop must enable fingerprint unlock, alongside Plasma's separate password authenticator";
assert lib.assertMsg (
  server.predator.hermes.enable
  && lib.all (
    c:
    lib.all (name: !(builtins.hasAttr name c.systemd.services)) [
      "hermes-agent"
      "hermes-browser-control"
      "camofox-browser"
      "hermes-network-isolation"
    ]
  ) (builtins.attrValues desktops)
) "Hermes and its browser must stay on Predator";
assert lib.assertMsg (
  builtins.elem "nct6775" main.boot.kernelModules
  && lib.hasInfix "fan-control prepare" cooling.text
  && lib.hasInfix "fan-control full-speed" cooling.text
  && lib.hasInfix "/bin/fancontrol /run/main-fan-control/fancontrol.conf" cooling.text
  && lib.hasInfix "fan-control watch /run/main-fan-control/fancontrol.conf" watch.text
  &&
    lib.all
      (
        host:
        !(host.systemd.units ? "fancontrol.service") && !(host.systemd.units ? "fan-profile-watch.service")
      )
      [
        laptop
        server
      ]
) "Only main may run die-temperature fancontrol and its power-profile watcher";
assert lib.assertMsg (
  lib.all (
    unit:
    !lib.hasInfix "power-profiles-daemon" (
      lib.concatStrings (lib.filter (line: !(lib.hasPrefix "#" line)) (lib.splitString "\n" unit.text))
    )
  ) [ cooling watch ]
) "Fan units must not depend on power-profiles-daemon: the daemon is itself ordered after multi-user.target, so ordering after it is a boot ordering cycle that makes systemd delete the fan start jobs";
assert lib.assertMsg (
  lib.all (unit: lib.hasInfix "StartLimitIntervalSec=0" unit.text) [ cooling watch ]
) "Fan units must disable the start rate limit: fan and watcher failsafes have to keep retrying instead of staying dead";
assert lib.assertMsg (
  laptop.hardware.cpu.amd.ryzen-smu.enable
  && laptop.systemd.services ? ryzenadj-laptop
  && !(main.systemd.services ? ryzenadj-laptop)
  && !(laptop.home-manager.users.mirsella.systemd.user.services ? ryzenadj-laptop)
  && flake.homeConfigurations.laptop.config.systemd.user.services ? ryzenadj-laptop
  && !(flake.homeConfigurations.main.config.systemd.user.services ? ryzenadj-laptop)
) "RyzenAdj must run only on laptop, once as a system service on NixOS or a user service on Arch";
assert lib.assertMsg (
  lib.toList laptop.systemd.services.ryzenadj-laptop.serviceConfig.StateDirectory == [ "ryzenadj-laptop" ]
  && lib.toList flake.homeConfigurations.laptop.config.systemd.user.services.ryzenadj-laptop.Service.StateDirectory == [ "ryzenadj-laptop" ]
  && lib.any (package: lib.getName package == "power-profiles-daemon") laptop.systemd.services.ryzenadj-laptop.path
) "RyzenAdj recovery must retain its cooldown across restarts and have powerprofilesctl available";
assert lib.assertMsg (
  lib.all
    (
      name:
      !(builtins.hasAttr name server.systemd.services)
      && !(builtins.hasAttr name server.home-manager.users.mirsella.systemd.user.services)
    )
    [
      "opencode"
      "openchamber"
    ]
  && lib.all (name: !(builtins.hasAttr name server.sops.secrets)) [
    "telegram_env"
    "opencode_server"
    "openchamber_server"
  ]
  &&
    lib.all
      (
        name:
        flake.homeConfigurations.main.config.systemd.user.services.${name}
        == flake.homeConfigurations.laptop.config.systemd.user.services.${name}
      )
      [
        "agent-tools-setup"
        "openchamber"
      ]
) "Agent setup and OpenChamber must share the workstation configuration; Predator must not run OpenCode or OpenChamber services";
assert lib.assertMsg (
  lib.hasPrefix "2." flake.packages.x86_64-linux.openchamber.version
  && lib.all (home: lib.any (
      package: package.outPath == flake.packages.x86_64-linux.openchamber.outPath
    ) home.home.packages) workstationHomes
) "Every workstation must use the flake's OpenChamber v2 package";
assert lib.assertMsg (lib.all
  (
    c:
    c.services.fail2ban.enable
    && c.services.fail2ban.maxretry == 3
    && c.services.fail2ban.bantime == "1m"
    && c.services.fail2ban.bantime-increment.enable
    && c.services.fail2ban.bantime-increment.maxtime == "1w"
    &&
      c.services.fail2ban.bantime-increment.formula
      == "ban.Time * (1 if ban.Count <= 0 else 5 * (1 << (ban.Count - 1)))"
    && c.services.fail2ban.jails.DEFAULT.settings.findtime == "10m"
    && c.services.fail2ban.jails.sshd.settings.enabled
    && builtins.elem "192.168.1.0/24" c.services.fail2ban.ignoreIP
  )
  (builtins.attrValues hosts)
) "Every host must ban SSH brute force after three failures and keep the LAN trusted";
assert lib.assertMsg (lib.all
  (
    name:
    let
      home = desktops.${name}.home-manager.users.mirsella;
      arch = flake.homeConfigurations.${name}.config;
    in
    home.programs.git.settings.user.signingkey == arch.programs.git.settings.user.signingkey
    && home.sops.secrets != { }
    && desktops.${name}.sops.secrets == { }
  )
  [
    "main"
    "laptop"
  ]
) "Desktop homes must share Git signing keys and install secrets only through Home Manager";
assert lib.assertMsg (
  lib.all (home:
    home.sops.secrets.codexbar_opencode_env.path == "${home.home.homeDirectory}/.config/codexbar/opencode.env"
    && home.sops.secrets.codexbar_opencode_env.mode == "0600"
    && home.home.file.".local/bin/codexbar".force
  ) workstationHomes
  && !(server.home-manager.users.mirsella.sops.secrets ? codexbar_opencode_env)
) "CodexBar must use a private Home Manager secret and launcher on workstations only";
assert lib.assertMsg
  (lib.all (
    home:
    !(home.systemd.user.services ? rclone-nextcloud)
    && !(home.sops.secrets ? davfs2_secrets)
    && home.sops.secrets.rclone_conf.path == "${home.home.homeDirectory}/.config/rclone/rclone.conf"
    && lib.hasInfix "Type=Link\n" home.xdg.dataFile."remoteview/nextcloud.desktop".text
    &&
      lib.hasInfix "URL=webdavs://mirsella@mirsella.mooo.com/nextcloud/remote.php/dav/files/mirsella/"
        home.xdg.dataFile."remoteview/nextcloud.desktop".text
  ) workstationHomes)
  "Nextcloud must use on-demand Dolphin WebDAV and direct rclone access, never an automatic FUSE mount";
assert lib.assertMsg
  (lib.all (
    home:
    let
      ui = home.systemd.user.services.openchamber;
    in
    !(home.sops.secrets ? opencode_server)
    && !(home.systemd.user.services ? opencode)
    && lib.hasPrefix "${lib.getExe flake.packages.x86_64-linux.openchamber} serve " (
      builtins.head ui.Service.ExecStart
    )
    && ui.Service.UnsetEnvironment == [ "OPENCODE_HOST" "OPENCODE_SKIP_START" ]
    && !(builtins.elem "opencode.service" ui.Unit.PartOf)
  ) workstationHomes)
  "OpenChamber must run its own managed OpenCode without the retired shared server";
assert lib.assertMsg (lib.all (
  home: lib.hasPrefix "/nix/store/" (builtins.head home.systemd.user.services.kache.Service.ExecStart)
) workstationHomes) "kache must run from the flake package on NixOS and Arch workstations";
assert lib.assertMsg
  (lib.all (
    home:
    lib.all
      (
        directory:
        home.xdg.configFile."opencode/${directory}".source
        == "${flake.packages.x86_64-linux.opencode-extensions}/share/opencode/${directory}"
      )
      [
        "plugins"
        "chunks"
      ]
    &&
      home.home.file.".local/bin/opencode-idle-watchdog".source
      == "${flake.packages.x86_64-linux.idle-watchdog}/bin/opencode-idle-watchdog"
    &&
      home.home.file.".local/bin/t3-idle-watchdog".source
      == "${flake.packages.x86_64-linux.idle-watchdog}/bin/t3-idle-watchdog"
  ) workstationHomes)
  "Home Manager must install the compiled OpenCode extensions and watchdogs on both operating systems";
assert lib.assertMsg
  (lib.all (
    home:
    let
      leanreaSkill = home.home.file.".config/opencode/skills/leanrea";
    in
    !home.programs.opencode.enable
    && !(builtins.hasAttr "opencode/opencode.json" home.xdg.configFile)
    && lib.elem flake.packages.x86_64-linux.electron-fuses home.home.packages
    && leanreaSkill.source == "${flake.packages.x86_64-linux.leanrea}/share/leanrea"
    && leanreaSkill.recursive
  ) workstationHomes)
  "Workstations must keep OpenCode JSONC canonical and install leanrea and electron-fuses without generating opencode.json";
assert lib.assertMsg
  (
    lib.all (
      home:
      let
        watch = home.systemd.user.services.electron-theme-watch;
      in
      lib.hasPrefix "${flake.packages.x86_64-linux.host-tools}/bin/host-tools theme-watch --dconf /nix/store/" (
        builtins.head watch.Service.ExecStart
      )
      && watch.Service.Restart == "on-failure"
      && !(watch.Service ? ExecStopPost)
      && lib.hasSuffix "setGtkTheme s Adwaita" watch.Service.ExecStartPre
      && lib.elem "plasma-kded6.service" watch.Unit.After
      && !(home.systemd.user.services ? electron-gtk-style)
    ) workstationHomes
    && !(server.home-manager.users.mirsella.systemd.user.services ? electron-theme-watch)
    && !(server.home-manager.users.mirsella.systemd.user.services ? electron-gtk-style)
  )
  "Plasma workstations must select Adwaita in the theme watcher without nudges or a separate style service; headless servers must run neither";
assert lib.assertMsg (lib.all
  (
    name:
    let
      nixosHome = desktops.${name}.home-manager.users.mirsella;
      archHome = flake.homeConfigurations.${name}.config;
    in
    !(lib.elem flake.packages.x86_64-linux.opencode nixosHome.home.packages)
    && !(lib.elem flake.packages.x86_64-linux.opencode archHome.home.packages)
    && !(nixosHome.systemd.user.services ? opencode)
  )
  [
    "main"
    "laptop"
  ]
) "The retired shared OpenCode service must be gone on every workstation";
assert lib.assertMsg (lib.all
  (
    config:
    let
      luks = config.boot.initrd.luks.devices;
      expected =
        name: partlabel:
        luks.${name}.device == "/dev/disk/by-partlabel/${partlabel}"
        && luks.${name}.allowDiscards
        && luks.${name}.crypttabExtraOpts == [ "tpm2-device=auto" ];
    in
    expected "cryptroot" "nixos-root"
    && expected "cryptswap" "nixos-swap"
    && config.boot.resumeDevice == "/dev/mapper/cryptswap"
    && config.fileSystems."/".device == "/dev/mapper/cryptroot"
    && config.fileSystems."/".fsType == "xfs"
    && builtins.map (s: s.device) config.swapDevices == [ "/dev/mapper/cryptswap" ]
    && config.services.fstrim.enable
    && builtins.elem "hibernate.compressor=lzo" config.boot.kernelParams
  )
  (builtins.attrValues desktops)
) "The desktops must be TPM2-unlocked XFS with LUKS-backed hibernation";
assert lib.assertMsg (
  lib.all (
    name:
    let
      home = flake.homeConfigurations.${name}.config;
      service = home.systemd.user.services.disk-alerts;
      timer = home.systemd.user.timers.disk-alerts;
    in
    lib.hasInfix "/bin/host-tools disk-alerts /" (builtins.head service.Service.ExecStart)
    && lib.toList service.Service.StateDirectory == [ "disk-alerts" ]
    && timer.Timer.OnUnitInactiveSec == "5min"
    && timer.Install.WantedBy == [ "timers.target" ]
    && !(desktops.${name}.home-manager.users.mirsella.systemd.user.services ? disk-alerts)
  ) [ "main" "laptop" ]
  && !(server.home-manager.users.mirsella.systemd.user.services ? disk-alerts)
) "Arch workstations must monitor disk and Btrfs allocation pressure without running it on XFS hosts";
{
  nixos = builtins.mapAttrs (_: c: c.system.build.toplevel.drvPath) desktops;
  arch = builtins.mapAttrs (_: home: home.activationPackage.drvPath) flake.homeConfigurations;
}
