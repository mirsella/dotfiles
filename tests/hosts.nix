# Evaluate the installed hardware layouts and host-role boundaries.
flake:
let
  inherit (flake.inputs.nixpkgs) lib;
  desktops = lib.genAttrs [ "main" "laptop" ] (name:
    flake.nixosConfigurations.${name}.config
  );
  server = flake.nixosConfigurations.predator.config;
  main = desktops.main;
  laptop = desktops.laptop;
  cooling = main.systemd.services.coolercontrold;
in
assert lib.assertMsg (
  builtins.attrNames flake.nixosModules == [ "laptop" "main" "predator" ]
  && builtins.attrNames flake.nixosConfigurations == [ "laptop" "main" "predator" ]
  && builtins.attrNames flake.homeConfigurations == [ "laptop" "main" ]
  && lib.all (name: desktops.${name}.networking.hostName == name) [ "main" "laptop" ]
) "Host discovery must select the correct hostname and standalone workstation homes";
assert lib.assertMsg (
  !server.services.desktopManager.plasma6.enable
  && !server.services.displayManager.sddm.enable
  && !server.programs.coolercontrol.enable
  && !server.hardware.cpu.amd.ryzen-smu.enable
  && lib.all (c:
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
  main.programs.coolercontrol.enable
  && !laptop.programs.coolercontrol.enable
  && builtins.elem "nct6775" main.boot.kernelModules
  && !(main.environment.etc ? "coolercontrol/config.toml")
  && cooling.serviceConfig.ConfigurationDirectory == "coolercontrol"
  && lib.hasInfix "/bin/install -m 0644 /nix/store/" cooling.serviceConfig.ExecStartPre
  && lib.hasSuffix " /etc/coolercontrol/config.toml" cooling.serviceConfig.ExecStartPre
  && cooling.restartIfChanged && cooling.stopIfChanged
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
  lib.all (name:
    !(builtins.hasAttr name server.systemd.services)
    && !(builtins.hasAttr name server.home-manager.users.mirsella.systemd.user.services)
  ) [ "opencode" "openchamber" ]
  && lib.all (name: !(builtins.hasAttr name server.sops.secrets)) [
    "telegram_env" "opencode_server" "openchamber_server"
  ]
  && lib.all (name:
    flake.homeConfigurations.main.config.systemd.user.services.${name}
    == flake.homeConfigurations.laptop.config.systemd.user.services.${name}
  ) [ "opencode" "openchamber" ]
) "OpenCode and OpenChamber must share the workstation configuration and stay off Predator";
assert lib.assertMsg (
  lib.all (name:
    let
      home = desktops.${name}.home-manager.users.mirsella;
      arch = flake.homeConfigurations.${name}.config;
    in
      lib.all (service:
        lib.hasPrefix "/nix/store/" (builtins.head home.systemd.user.services.${service}.Service.ExecStart)
        && lib.hasPrefix "/usr/bin/" (builtins.head arch.systemd.user.services.${service}.Service.ExecStart)
      ) [ "opencode" "openchamber" "rclone-nextcloud" ]
      && home.programs.git.settings.user.signingkey == arch.programs.git.settings.user.signingkey
      && home.sops.secrets != { }
      && desktops.${name}.sops.secrets == { }
      && builtins.elem "sops-nix.service" home.systemd.user.services.rclone-gdrive.Unit.After
      && builtins.elem "sops-nix.service" home.systemd.user.services.rclone-gdrive.Unit.Wants
      && lib.all (h:
        let unit = h.systemd.user.services.rclone-nextcloud;
        in lib.assertMsg (
          builtins.elem "sops-nix.service" unit.Unit.After
          && builtins.elem "sops-nix.service" unit.Unit.Wants
          && !(builtins.elem "sops-nix.service" (unit.Unit.Requires or [ ]))
          && unit.Service.Type == "notify"
          && !(unit.Service ? ExecStop)
          && unit.Service.SuccessExitStatus == "143"
        ) "WebDAV must wait for credentials and let rclone own mount readiness and shutdown"
      ) [ home arch ]
  ) [ "main" "laptop" ]
) "Desktop homes must use the right binaries and one user-level secret installer on both operating systems";
{
  nixos = builtins.mapAttrs (_: c: c.system.build.toplevel.drvPath) desktops;
  arch = builtins.mapAttrs (_: home: home.activationPackage.drvPath) flake.homeConfigurations;
}
