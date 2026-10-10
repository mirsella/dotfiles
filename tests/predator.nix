# Evaluate only; never build, activate, mount, or format a disk.
flake:
let
  inherit (flake.inputs.nixpkgs) lib;
  server = flake.nixosConfigurations.predator.config;
  layout = import ../disko.nix;
  disks = layout.disko.devices.disk;
  physical = import ../hosts/predator/disks.nix;
  hddNames = builtins.attrNames physical.hdds;
  smartOptions = builtins.listToAttrs (map (d: lib.nameValuePair d.device d.options)
    server.services.smartd.devices);
  ncdata = layout.disko.devices.zpool.fast.datasets.ncdata;
  pkgs = flake.inputs.nixpkgs.legacyPackages.x86_64-linux;
  utils = import (flake.inputs.nixpkgs + "/nixos/lib/utils.nix") {
    inherit lib pkgs;
    config = server;
  };
  disko = import flake.inputs.disko { inherit lib; };
in
assert lib.assertMsg (
  server.boot.lanzaboote.enable
  && server.boot.initrd.systemd.enable
  && server.boot.initrd.secrets == { }
  && lib.all (d: d.keyFile == null && builtins.elem "tpm2-device=auto" d.crypttabExtraOpts)
    (builtins.attrValues server.boot.initrd.luks.devices)
  && lib.all (fs: !fs.autoFormat) (builtins.attrValues server.fileSystems)
) "Installed system must use signed TPM boot without embedded keys or automatic formatting";
assert lib.assertMsg (
  lib.all (name:
    let unit = "systemd-cryptsetup@${utils.escapeSystemdPath "${name}-crypt"}.service";
    in builtins.elem unit server.systemd.services.zfs-import-tank.wants
      && builtins.elem unit server.systemd.services.zfs-import-tank.after
      && !(builtins.elem unit server.systemd.services.zfs-import-tank.requires)
  ) hddNames
  && server.systemd.services."systemd-cryptsetup@".serviceConfig.TimeoutSec == "2min"
) "HDD unlocking must be bounded and parallel, allowing degraded import when a member fails";
assert lib.assertMsg (
  builtins.attrNames server.boot.initrd.luks.devices == [ "crypt-root" ]
  && lib.hasInfix "fast-crypt ${disks.ssd.device}-part1 - tpm2-device=auto,discard,nofail,headless=true,x-systemd.device-timeout=30s"
    server.environment.etc.crypttab.text
  && builtins.elem "systemd-cryptsetup@fast\\x2dcrypt.service" server.systemd.services.zfs-import-fast.requires
  && ncdata.options.mountpoint == "legacy"
  && lib.all (dataset:
    dataset.options.mountpoint == "legacy"
    && builtins.elem "nofail" dataset.mountOptions
    && builtins.elem "nofail" server.fileSystems.${dataset.mountpoint}.options
  ) ([ layout.disko.devices.zpool.fast.datasets.data ncdata ]
    ++ map (name: layout.disko.devices.zpool.tank.datasets.${name}) [ "archive" "backup" "backup/recovery" ])
  && server.fileSystems.${ncdata.mountpoint}.device == "fast/ncdata"
  && lib.all (d:
    let luks = d.content.partitions.crypt.content;
    in luks.askPassword && !luks.initrdUnlock && !(luks.settings ? keyFile)
  ) (builtins.attrValues disks)
) "Fresh provisioning must match runtime mounts and leave credential enrollment explicit";
assert lib.assertMsg (
  layout.disko.devices.zpool.tank.mode == "raidz"
  && layout.disko.devices.zpool.tank.options.autoexpand == "on"
  && layout.disko.devices.zpool.tank.options.autoreplace == "off"
  && layout.disko.devices.zpool.tank.options.ashift == "12"
  && hddNames == [ "tank1" "tank2" "tank3" ]
  && builtins.attrNames disks == [ "ssd" "tank1" "tank2" "tank3" ]
  && lib.all (name:
    disks.${name}.device == physical.hdds.${name}
    && disks.${name}.content.partitions.crypt.content.name == "${name}-crypt"
    && lib.hasInfix "${name}-crypt ${disks.${name}.device}-part1 /etc/luks/${name}.key nofail,headless=true,x-systemd.device-timeout=30s"
      server.environment.etc.crypttab.text
  ) hddNames
) "Three-disk RAIDZ1 must provision and unlock every HDD using the same physical identities";
assert lib.assertMsg (
  lib.sort builtins.lessThan (map (d: d.device) server.services.smartd.devices)
    == lib.sort builtins.lessThan (builtins.attrValues physical.ssds ++ builtins.attrValues physical.hdds)
  && builtins.attrNames smartOptions
    == lib.sort builtins.lessThan server.services.beszel.agent.smartmon.deviceAllow
  && server.systemd.services.smartd.serviceConfig.Restart == "on-failure"
  && server.services.zfs.zed.settings.ZED_NOTIFY_DATA
) "SMART and Beszel must monitor every physical disk and ZED must report ZFS errors";
assert lib.assertMsg (
  !(lib.hasInfix "-W" server.services.smartd.defaults.monitored)
  && lib.hasInfix "-d removable" server.services.smartd.defaults.monitored
  && lib.all (device:
    lib.hasInfix "-W 0,0,65" smartOptions.${device}
    && lib.hasInfix "-s (S/../.././12|L/../../7/13)" smartOptions.${device}
  ) (builtins.attrValues physical.ssds)
  && lib.all (name:
    let options = smartOptions.${physical.hdds.${name}};
        schedule = {
          tank1 = "S/../.././12|L/../../7/14";
          tank2 = "S/../.././12|L/../../7/15";
          tank3 = "S/../.././12|L/../../7/13";
        }.${name};
    in lib.hasInfix "-W 0,0,55" options
      && lib.hasInfix "-s (${schedule})" options
  ) hddNames
  && server.services.zfs.autoScrub.enable
  && server.services.zfs.autoScrub.pools == [ "fast" "tank" ]
) "SMART must schedule daily short tests, weekly long tests, high-temperature-only warnings and pool scrubs";
assert lib.assertMsg (
  server.services.syncoid.enable
  && server.services.syncoid.interval == "23:45"
  && builtins.elem "--no-rollback" server.services.syncoid.commonArgs
  && lib.all (permission: builtins.elem permission server.services.syncoid.localTargetAllow)
    ([ "readonly" "canmount" "acltype" "aclinherit" "destroy" ]
      ++ flake.nixosConfigurations.predator.options.services.syncoid.localTargetAllow.default)
  && layout.disko.devices.zpool.tank.datasets.replica.options
    == { mountpoint = "none"; canmount = "off"; readonly = "on"; }
  && !server.services.sanoid.datasets."tank/replica".autosnap
  && lib.all (name: server.services.sanoid.datasets.${name}.autosnap)
    [ "fast/data" "fast/ncdata" "tank/archive" "tank/backup" ]
  && lib.all (dataset: dataset.recursive) (builtins.attrValues server.services.sanoid.datasets)
  && lib.all (name:
    let command = server.services.syncoid.commands.${name};
    in command.source == "fast/${name}" && command.target == "tank/replica/${name}"
      && command.recursive && command.sendOptions == "Lc p"
      && command.recvOptions == "u o mountpoint=none o canmount=off o readonly=on"
      && server.systemd.timers."syncoid-${name}".timerConfig.Persistent
  ) [ "data" "ncdata" ]
) "SSD data must replicate nightly to unmounted read-only HDD datasets, without destination snapshots";
assert lib.assertMsg (
  lib.all (name:
    server.systemd.services.${name}.serviceConfig.UMask == "0077"
    && server.systemd.timers.${name}.timerConfig.Persistent
    && server.systemd.services.${name}.unitConfig.RequiresMountsFor == [ "/srv/backup/recovery" ]
  ) [ "db-backup" "recovery-backup" ]
  && lib.toList server.systemd.timers.recovery-backup.timerConfig.OnCalendar == [ "23:10" ]
  && lib.toList server.systemd.timers.db-backup.timerConfig.OnCalendar == [ "23:00" ]
  && server.fileSystems."/srv/backup/recovery".device == "tank/backup/recovery"
  && layout.disko.devices.zpool.tank.datasets."backup/recovery".mountpoint == "/srv/backup/recovery"
  && layout.disko.devices.zpool.tank.datasets."backup/recovery".options.mountpoint == "legacy"
  && builtins.elem "nofail" server.fileSystems."/srv/backup/recovery".options
) "Database and recovery bundles must share a private legacy mount without blocking boot on failure";
assert lib.assertMsg (
  lib.toList server.systemd.services.hermes-backup.startAt == [ "23:20" ]
  && server.systemd.services.hermes-backup.unitConfig.RequiresMountsFor == [ "/srv/backup" ]
  && server.systemd.services.hermes-backup.serviceConfig.UMask == "0077"
  && server.systemd.timers.hermes-backup.timerConfig.Persistent
  && server.fileSystems."/srv/backup".device == "tank/backup"
  && lib.any (path: lib.hasInfix "util-linux" (toString path)) server.systemd.services.hermes-backup.path
  && builtins.elem pkgs.gzip server.systemd.services.hermes-backup.path
) "Hermes runtime state must be backed up daily to the mounted Tank dataset";
assert lib.assertMsg (
  server.systemd.services.caddy.serviceConfig.Restart == "on-failure"
) "The server's web proxy must retry transient startup failures";
assert lib.assertMsg (
  server.services.hermes-agent.user == "mirsella"
  && server.services.hermes-agent.group == "hermes-private"
  && server.users.groups.hermes-private.members == [ "mirsella" ]
  && server.users.users.mirsella.linger
  && !server.services.hermes-agent.createUser
   && server.services.hermes-agent.settings.skills.external_dirs == [ "/home/mirsella/.agents/skills" "/home/mirsella/.codex/skills" ]
   && server.services.hermes-agent.settings.model.provider == "opencode-go"
   && server.services.hermes-agent.settings.model.default == "muse-spark-1.3-contributor"
   && server.services.hermes-agent.settings.fallback_providers == []
   && server.services.hermes-agent.settings.custom_providers == []
   && server.services.hermes-agent.settings.providers.opencode-go.enabled
   && lib.all (name: !server.services.hermes-agent.settings.providers.${name}.enabled)
     [ "openrouter" "openai" "openai-api" "openai-codex" "chatgpt" "opencode" "opencode-zen" "zen" "custom" "auto" "anthropic" ]
   && lib.all (slot: slot.provider == "opencode-go" && slot.model == "muse-spark-1.3-contributor" && slot.api_key == "" && slot.base_url == "")
     (builtins.attrValues (builtins.removeAttrs server.services.hermes-agent.settings.auxiliary [ "openrouter_model" ]))
  && lib.elem "hermes-agent-setup" server.system.activationScripts.hermes-settings.deps
  && lib.hasInfix "runuser -u mirsella -g hermes-private" server.system.activationScripts.hermes-settings.text
  && lib.hasInfix "hermes-configure" server.system.activationScripts.hermes-settings.text
  && server.systemd.services.hermes-agent.environment.HOME == "/home/mirsella"
  && server.systemd.services.hermes-agent.environment.HERMES_MANAGED == "false"
  && server.services.hermes-agent.environment.OPENCODE_GO_BASE_URL == "http://127.0.0.1:17321/sleev/hermes/opencode-go"
   && !(server.services.hermes-agent.environment ? OPENCODE_ZEN_BASE_URL)
   && !(server.services.hermes-agent.environment ? OPENAI_BASE_URL)
   && !(server.services.hermes-agent.environment ? HERMES_CODEX_BASE_URL)
   && lib.all (name: lib.elem name server.systemd.services.hermes-agent.serviceConfig.UnsetEnvironment && lib.elem name server.systemd.services.hermes-backend.serviceConfig.UnsetEnvironment)
     [ "OPENROUTER_API_KEY" "OPENAI_API_KEY" "OPENCODE_ZEN_API_KEY" "DEEPINFRA_API_KEY" "MOONSHOT_API_KEY" ]
  && server.services.hermes-agent.environment.TELEGRAM_ALLOW_ALL_USERS == "false"
  && server.services.hermes-agent.extraPlugins == []
  && server.services.hermes-agent.package.hermesVenv.drvPath
    == flake.inputs.hermes-agent.packages.x86_64-linux.messaging.hermesVenv.drvPath
  && server.services.hermes-agent.package.hermesWeb.drvPath
    != flake.inputs.hermes-agent.packages.x86_64-linux.messaging.hermesWeb.drvPath
  && lib.hasInfix "base: \"/hermes/\"" server.services.hermes-agent.package.hermesWeb.postPatch
  && server.services.hermes-agent.documents ? "AGENTS.md"
  && !(server.services.hermes-agent.settings ? hooks)
  && !(server.services.hermes-agent.settings ? approvals)
  && !(server.services.hermes-agent.settings ? toolsets)
  && server.sops.secrets.hermes_gateway_env.owner == "mirsella"
  && server.sops.secrets.hermes_gateway_env.mode == "0400"
   && server.sops.secrets.hermes_gateway_env.restartUnits == [ "hermes-backend.service" "hermes-agent.service" ]
  && server.sops.secrets.hermes_control_env.restartUnits == [ "hermes-browser-control.service" "hermes-agent.service" ]
  && builtins.elem "sops-install-secrets.service" server.systemd.services.hermes-agent.requires
  && lib.hasInfix server.sops.secrets.hermes_gateway_env.path server.systemd.services.hermes-agent.preStart
  && !server.systemd.services.hermes-agent.serviceConfig.NoNewPrivileges
  && !server.systemd.services.hermes-agent.serviceConfig.ProtectHome
  && !server.systemd.services.hermes-agent.serviceConfig.ProtectSystem
  && !server.systemd.services.hermes-agent.serviceConfig.PrivateTmp
  && builtins.elem "sleev-gateway.service" server.systemd.services.hermes-agent.requires
  && builtins.elem "hermes-secret-service.service" server.systemd.services.hermes-agent.requires
  && server.systemd.services.hermes-secret-service.wantedBy == [ "multi-user.target" ]
  && lib.all (unit: builtins.elem unit server.systemd.services.hermes-secret-service.requires
    && builtins.elem unit server.systemd.services.hermes-secret-service.after)
    [ "user@1000.service" "sops-install-secrets.service" ]
  && server.systemd.services.hermes-secret-service.environment.DBUS_SESSION_BUS_ADDRESS
    == "unix:path=/run/user/1000/bus"
  && server.systemd.services.hermes-secret-service.serviceConfig.ExecStart
    == "${pkgs.gnome-keyring}/bin/gnome-keyring-daemon --foreground --unlock --components=secrets"
  && server.systemd.services.hermes-secret-service.serviceConfig.StandardInput
    == "file:${server.sops.secrets.hermes_keyring_password.path}"
  && lib.hasSuffix "wait --session --timeout 30 org.freedesktop.secrets"
    server.systemd.services.hermes-secret-service.serviceConfig.ExecStartPost
  && server.systemd.services.sleev-gateway.wantedBy == [ "multi-user.target" ]
  && server.sops.secrets.hermes_keyring_password.owner == "mirsella"
  && server.sops.secrets.hermes_keyring_password.mode == "0400"
  && server.systemd.services.hermes-agent.environment.PROTON_PASS_LINUX_KEYRING == "dbus"
  && server.systemd.services.hermes-backend.environment == server.systemd.services.hermes-agent.environment
  && !(server.sops.secrets ? hermes_pass_token)
  && !(server.systemd.services ? hermes-pass-login)
  && !(server.systemd.timers ? hermes-pass-login)
  && server.systemd.services.sleev-gateway.serviceConfig.User == "mirsella"
  && server.systemd.services.hermes-agent.serviceConfig.BindReadOnlyPaths
    == [ "/var/lib/camofox-downloads:/var/lib/hermes/workspace/downloads" ]
  && lib.all (unit:
    server.systemd.services.${unit}.serviceConfig.NoNewPrivileges
    && server.systemd.services.${unit}.serviceConfig.ProtectHome != false
    && server.systemd.services.${unit}.serviceConfig.ProtectSystem == "strict"
  ) [ "hermes-browser-control" "camofox-browser" ]
  && server.systemd.services.camofox-browser.environment.CAMOFOX_BIND_HOST == "127.0.0.1"
  && server.systemd.services.camofox-browser.environment.VNC_RFB_BIND == "127.0.0.1"
  && server.systemd.services.camofox-browser.environment.NOVNC_PORT == "6080"
  && server.systemd.services.camofox-browser.environment.CAMOUFOX_INSTALL_DIR
    == "${flake.nixosConfigurations.predator.pkgs.camoufox}/lib/camoufox"
  && lib.all (name: lib.any (package: lib.getName package == name)
    server.systemd.services.camofox-browser.path) [ "which" "gawk" ]
  && lib.all (port: !(builtins.elem port server.networking.firewall.allowedTCPPorts))
     [ 5900 6080 9377 9378 9119 ]
  && lib.all (text: lib.hasInfix text server.services.caddy.virtualHosts."mirsella.mooo.com".extraConfig)
    [ "handle /browser/*" "basic_auth" "header_up X-Hermes-Viewer-Key" "header_up -Authorization" ]
  && lib.hasInfix "https://mirsella.mooo.com{uri}" server.services.caddy.virtualHosts."http://:80".extraConfig
) "Hermes must use the owner account and editable runtime settings while its browser stays private";
assert lib.assertMsg (
  server.services.hermes-agent.backend.mode == "dashboard"
  && server.services.hermes-agent.backend.host == "127.0.0.1"
  && server.services.hermes-agent.backend.port == 9119
  && server.services.hermes-agent.backend.extraArgs == [ "--skip-build" ]
  && server.systemd.services.hermes-backend.wantedBy == [ "multi-user.target" ]
  && server.systemd.services.hermes-backend.environment.HOME == "/home/mirsella"
  && server.systemd.services.hermes-backend.environment.HERMES_MANAGED == "false"
  && !server.systemd.services.hermes-backend.serviceConfig.NoNewPrivileges
  && !server.systemd.services.hermes-backend.serviceConfig.ProtectSystem
  && lib.elem "hermes-network-isolation.service" server.systemd.services.hermes-backend.requires
  && lib.elem "network-online.target" server.systemd.services.caddy.wants
  && lib.all (text: lib.hasInfix text server.services.caddy.virtualHosts."mirsella.mooo.com".extraConfig)
    [ "handle_path /hermes/*" "@foreignOrigin" "header_up X-Forwarded-Prefix /hermes" "header_up Origin http://127.0.0.1:9119" ]
) "Hermes dashboard must boot privately and authenticate every proxy route with a fixed subpath and checked Origin";
{
  inherit (server.system.build.toplevel) drvPath;
  formatter = (disko._cliDestroyFormatMount layout pkgs).drvPath;
}
