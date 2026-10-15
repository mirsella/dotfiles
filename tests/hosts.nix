# Evaluate the installed hardware layouts and host-role boundaries.
flake:
let
  inherit (flake.inputs.nixpkgs) lib;
  hosts = lib.mapAttrs (_: host: host.config) flake.nixosConfigurations;
  desktops = { inherit (hosts) main laptop; };
  server = hosts.predator;
  main = hosts.main;
  laptop = hosts.laptop;
  cooling = main.systemd.services.coolercontrold;
  workstationHomes = lib.concatMap (name: [
    desktops.${name}.home-manager.users.mirsella
    flake.homeConfigurations.${name}.config
  ]) (builtins.attrNames desktops);
  agentHomes = workstationHomes ++ [ server.home-manager.users.mirsella ];
  cacheWarmer = lib.getExe flake.packages.x86_64-linux.t3-cache-warmer;
in
assert lib.assertMsg (lib.all (
  home:
  home.home.file.".local/bin/t3-cache-warmer".source == cacheWarmer
  && home.systemd.user.services.t3-cache-warmer.Service.ExecStart
  == [ "${cacheWarmer} --config %h/.config/t3-cache-warmer/config.json run" ]
) agentHomes) "Every T3 installation must run the cache warmer with its provider allowlist config";
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
  && !server.programs.coolercontrol.enable
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
  main.programs.coolercontrol.enable
  && !laptop.programs.coolercontrol.enable
  && builtins.elem "nct6775" main.boot.kernelModules
  && !(main.environment.etc ? "coolercontrol/config.toml")
  && cooling.serviceConfig.ConfigurationDirectory == "coolercontrol"
  && lib.match ".*/install -m 0644 -t /etc/coolercontrol [^ ]*config\\.toml [^ ]*modes\\.json" cooling.serviceConfig.ExecStartPre != null
  && cooling.restartIfChanged
  && cooling.stopIfChanged
  && !(cooling.serviceConfig ? stopIfChanged)
  && !(cooling.serviceConfig ? restartIfChanged)
  && lib.hasInfix "ExecStartPre=" main.systemd.units."coolercontrold.service".text
) "Only main may apply its fan curve, through the daemon's stop/start lifecycle";
assert lib.assertMsg (
  laptop.hardware.cpu.amd.ryzen-smu.enable
  && laptop.systemd.services ? ryzenadj-laptop
  && !(main.systemd.services ? ryzenadj-laptop)
  && !(laptop.home-manager.users.mirsella.systemd.user.services ? ryzenadj-laptop)
  && flake.homeConfigurations.laptop.config.systemd.user.services ? ryzenadj-laptop
  && !(flake.homeConfigurations.main.config.systemd.user.services ? ryzenadj-laptop)
) "RyzenAdj must run only on laptop, once as a system service on NixOS or a user service on Arch";
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
      == "${flake.packages.x86_64-linux.opencode-idle-watchdog}/bin/opencode-idle-watchdog"
  ) workstationHomes)
  "Home Manager must install the compiled OpenCode extensions and watchdog on both operating systems";
assert lib.assertMsg
  (lib.all (
    home:
    let
      reaSkill = home.home.file.".config/opencode/skills/reverse-engineer-anything";
    in
    !home.programs.opencode.enable
    && !(builtins.hasAttr "opencode/opencode.json" home.xdg.configFile)
    && lib.elem flake.packages.x86_64-linux.rea home.home.packages
    &&
      reaSkill.source
      == "${flake.packages.x86_64-linux.rea}/lib/node_modules/rea-agents/skills/reverse-engineer-anything"
    && reaSkill.recursive
  ) workstationHomes)
  "Workstations must keep OpenCode JSONC canonical and install the REA skill without generating opencode.json";
assert lib.assertMsg (lib.all (
  home:
  let
    watch = home.systemd.user.services.electron-theme-watch;
  in
  lib.hasPrefix
    "${flake.packages.x86_64-linux.host-tools}/bin/host-tools theme-watch --dconf /nix/store/"
    (builtins.head watch.Service.ExecStart)
  && watch.Service.Restart == "on-failure"
  && lib.hasSuffix "--recover" watch.Service.ExecStopPost
) workstationHomes
&& !(server.home-manager.users.mirsella.systemd.user.services ? electron-theme-watch)
) "The theme watcher must run the flake host-tools, recover after nudge failures, and stay off the headless server";
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
{
  nixos = builtins.mapAttrs (_: c: c.system.build.toplevel.drvPath) desktops;
  arch = builtins.mapAttrs (_: home: home.activationPackage.drvPath) flake.homeConfigurations;
}
