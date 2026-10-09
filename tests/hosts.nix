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
in
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
  && lib.hasInfix "/bin/install -m 0644 /nix/store/" cooling.serviceConfig.ExecStartPre
  && lib.hasSuffix " /etc/coolercontrol/config.toml" cooling.serviceConfig.ExecStartPre
  && cooling.restartIfChanged
  && cooling.stopIfChanged
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
        "opencode"
        "openchamber"
      ]
) "OpenCode and OpenChamber must share the workstation configuration and stay off Predator";
assert lib.assertMsg (lib.all
  (
    name:
    lib.any (
      package: lib.hasPrefix "openchamber-1." package.name
    ) desktops.${name}.home-manager.users.mirsella.home.packages
  )
  [
    "main"
    "laptop"
  ]
) "OpenChamber must stay on 1.x while the workstations run OpenCode 1.x";
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
assert lib.assertMsg
  (lib.all
    (
      name:
      let
        home = desktops.${name}.home-manager.users.mirsella;
        arch = flake.homeConfigurations.${name}.config;
      in
      lib.all
        (
          service:
          lib.hasPrefix "/nix/store/" (builtins.head home.systemd.user.services.${service}.Service.ExecStart)
          && lib.hasPrefix "/usr/bin/" (builtins.head arch.systemd.user.services.${service}.Service.ExecStart)
        )
        [
          "opencode"
          "openchamber"
          "rclone-nextcloud"
        ]
      && home.programs.git.settings.user.signingkey == arch.programs.git.settings.user.signingkey
      && home.sops.secrets != { }
      && desktops.${name}.sops.secrets == { }
      &&
        lib.all
          (
            h:
            let
              unit = h.systemd.user.services.rclone-nextcloud;
            in
            lib.assertMsg (
              builtins.elem "sops-nix.service" unit.Unit.After
              && builtins.elem "sops-nix.service" unit.Unit.Wants
              && !(builtins.elem "sops-nix.service" (unit.Unit.Requires or [ ]))
              && unit.Service.Type == "notify"
              && !(unit.Service ? ExecStop)
              && unit.Service.SuccessExitStatus == "143"
            ) "WebDAV must wait for credentials and let rclone own mount readiness and shutdown"
          )
          [
            home
            arch
          ]
    )
    [
      "main"
      "laptop"
    ]
  )
  "Desktop homes must use the right binaries and one user-level secret installer on both operating systems";
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
        "tui-plugins"
        "chunks"
      ]
    &&
      home.home.file.".local/bin/opencode-idle-watchdog".source
      == "${flake.packages.x86_64-linux.opencode-idle-watchdog}/bin/opencode-idle-watchdog"
  ) workstationHomes)
  "Home Manager must install the compiled OpenCode extensions and watchdog on both operating systems";
assert lib.assertMsg (lib.all
  (
    home:
    let
      rea = home.programs.opencode.settings.mcp.rea;
    in
    home.programs.opencode.enable
    && rea.type == "local"
    &&
      rea.command == [
        "${flake.packages.x86_64-linux.rea}/bin/rea"
        "mcp"
      ]
    && !rea.enabled
    &&
      home.programs.opencode.skills.reverse-engineer-anything
      == "${flake.packages.x86_64-linux.rea}/lib/node_modules/rea-agents/skills/reverse-engineer-anything"
  )
  workstationHomes
) "REA must use the pinned Nix runtime and stay disabled by default on every workstation profile";
assert lib.assertMsg (
  let
    provider = flake.homeConfigurations.main.config.programs.opencode.settings.provider;
  in
  provider ? "llama.cpp"
  && provider == main.home-manager.users.mirsella.programs.opencode.settings.provider
) "The local model provider must coexist with REA on both main profiles";
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
