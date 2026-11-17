# Evaluate only; never build, activate, mount, or format a disk.
flake:
let
  inherit (flake.inputs.nixpkgs) lib;
  server = flake.nixosConfigurations.predator.config;
  layout = import ../disko.nix;
  disks = layout.disko.devices.disk;
  physical = import ../hosts/predator/disks.nix;
  hddNames = builtins.attrNames physical.hdds;
  backupNames = builtins.attrNames physical.backupHdds;
  dataNames = [ "fast" ] ++ hddNames ++ backupNames;
  smartOptions = builtins.listToAttrs (map (d: lib.nameValuePair d.device d.options)
    server.services.smartd.devices);
  capacityPolicy = builtins.fromJSON (
    builtins.unsafeDiscardStringContext server.environment.etc."zfs-capacity.json".text
  );
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
  && builtins.attrValues server.boot.initrd.secrets
    == [ "/var/lib/predator-boot/recovery-ssh/host-key.cred" ]
  && server.boot.initrd.network.ssh.enable
  && server.boot.initrd.network.ssh.port == 2223
  && server.boot.initrd.network.ssh.hostKeys == []
  && lib.hasInfix "HostKey /run/predator-recovery/host-key" server.boot.initrd.network.ssh.extraConfig
  && builtins.elem "predator-recovery-key.service" server.boot.initrd.systemd.services.sshd.requires
  && builtins.elem "systemd-pcrphase-initrd.service" server.boot.initrd.systemd.services.predator-recovery-key.after
  && builtins.elem "systemd-pcrosseparator.service" server.boot.initrd.systemd.services.predator-recovery-key.after
  && !(builtins.elem "systemd-cryptsetup@crypt\\x2droot.service" server.boot.initrd.systemd.services.predator-recovery-key.requires)
  && !(builtins.elem "systemd-cryptsetup@crypt\\x2droot.service" server.boot.initrd.systemd.services.predator-recovery-key.after)
  && !(builtins.elem "systemd-cryptsetup@crypt\\x2droot.service" server.boot.initrd.systemd.services.sshd.after)
  && builtins.elem "initrd-nixos-copy-secrets.service" server.boot.initrd.systemd.services.predator-recovery-key.after
  && builtins.elem "initrd.target" server.boot.initrd.systemd.services.predator-recovery-key.before
  && builtins.elem "initrd.target" server.boot.initrd.systemd.services.sshd.before
  && server.boot.initrd.systemd.services.predator-recovery-key.serviceConfig.TimeoutStartSec == "30s"
  && server.boot.initrd.systemd.services.sshd.serviceConfig.KillMode == "control-group"
  && builtins.elem "initrd-switch-root.target" server.boot.initrd.systemd.services.sshd.conflicts
  && lib.all (d: d.keyFile == null && builtins.elem "tpm2-device=auto" d.crypttabExtraOpts)
    (builtins.attrValues server.boot.initrd.luks.devices)
  && lib.all (fs: !fs.autoFormat) (builtins.attrValues server.fileSystems)
) "Installed system must use signed TPM boot with only an encrypted recovery SSH identity, no embedded disk keys, and no automatic formatting";
assert lib.assertMsg (
  server.boot.initrd.systemd.services.systemd-pcrosseparator.enable
  && server.boot.initrd.systemd.services.systemd-pcrosseparator.unitConfig.ConditionSecurity == "measured-os"
  && builtins.elem "sysinit.target" server.boot.initrd.systemd.services.systemd-pcrosseparator.wantedBy
  && builtins.elem "cryptsetup-pre.target" server.boot.initrd.systemd.services.systemd-pcrosseparator.before
  && lib.hasInfix "--event-type=os-separator os-separator"
    server.boot.initrd.systemd.services.systemd-pcrosseparator.serviceConfig.ExecStart
) "Authenticated boot must separate firmware authorization before root unlock";
assert lib.assertMsg (
  server.boot.initrd.systemd.tpm2.pcrphases.enable
  && server.systemd.tpm2.pcrphases.enable
  && builtins.elem "predator.authenticated-boot=1" server.boot.kernelParams
  && builtins.elem "tpm2-pcrs=7" server.boot.initrd.luks.devices.crypt-root.crypttabExtraOpts
  && server.boot.initrd.systemd.services."systemd-cryptsetup@crypt\\x2droot".serviceConfig.BindReadOnlyPaths
    == [ "/run/credentials/@encrypted/predator-pcr-signature:/run/systemd/tpm2-pcr-signature.json" ]
  && server.systemd.services."systemd-cryptsetup@".serviceConfig.BindReadOnlyPaths
    == server.boot.initrd.systemd.services."systemd-cryptsetup@crypt\\x2droot".serviceConfig.BindReadOnlyPaths
  && lib.all (name:
    builtins.elem "systemd-cryptsetup@${utils.escapeSystemdPath "${name}-crypt"}.service"
      server.systemd.services.systemd-pcrphase-sysinit.after
   ) dataNames
) "Root and data unlocks must use distinct, explicitly ordered PCR11 phases";
assert lib.assertMsg (
  lib.all (name:
    let unit = "systemd-cryptsetup@${utils.escapeSystemdPath "${name}-crypt"}.service";
    in builtins.elem unit server.systemd.services.zfs-import-tank.wants
      && builtins.elem unit server.systemd.services.zfs-import-tank.after
      && !(builtins.elem unit server.systemd.services.zfs-import-tank.requires)
  ) hddNames
  && lib.all (name:
    let unit = "systemd-cryptsetup@${utils.escapeSystemdPath "${name}-crypt"}.service";
    in builtins.elem unit server.systemd.services.zfs-import-backups.wants
      && builtins.elem unit server.systemd.services.zfs-import-backups.after
      && !(builtins.elem unit server.systemd.services.zfs-import-backups.requires)
  ) backupNames
  && server.systemd.services."systemd-cryptsetup@".serviceConfig.TimeoutSec == "2min"
) "HDD unlocking must be bounded and parallel, allowing degraded import when a member fails";
assert lib.assertMsg (
  lib.all (pool:
    let service = server.systemd.services."zfs-import-${pool}";
    in service.serviceConfig.ExecStop
        == "${server.boot.zfs.package}/bin/zpool export ${lib.escapeShellArg pool}"
      && server.boot.zfs.pools.${pool}.devNodes == "/dev/mapper"
      && !service.restartIfChanged
      && !service.stopIfChanged
      && service.serviceConfig.RemainAfterExit
      && builtins.elem "shutdown.target" service.conflicts
      && lib.all (dataset:
        builtins.elem "${utils.escapeSystemdPath dataset.mountpoint}.mount" service.before
      ) (builtins.attrValues layout.disko.devices.zpool.${pool}.datasets)
  ) [ "backups" "fast" "tank" ]
) "Every data pool must export after its mounts stop and before LUKS detach, without exporting during live switches";
assert lib.assertMsg (
  builtins.attrNames server.boot.initrd.luks.devices == [ "crypt-root" ]
  && lib.hasInfix "fast-crypt ${disks.ssd.device}-part1 - tpm2-device=auto,tpm2-pcrs=7,tpm2-signature="
    server.environment.etc.crypttab.text
  && builtins.elem "systemd-cryptsetup@fast\\x2dcrypt.service" server.systemd.services.zfs-import-fast.requires
  && ncdata.options.mountpoint == "legacy"
  && lib.all (dataset:
    dataset.options.mountpoint == "legacy"
    && builtins.elem "nofail" dataset.mountOptions
    && builtins.elem "nofail" server.fileSystems.${dataset.mountpoint}.options
  ) ([ layout.disko.devices.zpool.fast.datasets.data ncdata ]
    ++ [ layout.disko.devices.zpool.tank.datasets.archive ]
    ++ builtins.attrValues layout.disko.devices.zpool.backups.datasets)
  && server.fileSystems.${ncdata.mountpoint}.device == "fast/ncdata"
  && lib.all (d:
    lib.all (partition:
      let luks = partition.content;
      in luks.askPassword && !luks.initrdUnlock && !(luks.settings ? keyFile)
    ) (builtins.attrValues d.content.partitions)
  ) (builtins.attrValues disks)
) "Fresh provisioning must match runtime mounts and leave credential enrollment explicit";
assert lib.assertMsg (
  layout.disko.devices.zpool.tank.mode == "raidz"
  && layout.disko.devices.zpool.tank.options.autoexpand == "on"
  && layout.disko.devices.zpool.tank.options.autoreplace == "off"
  && layout.disko.devices.zpool.tank.options.ashift == "12"
  && hddNames == [ "tank1" "tank2" "tank3" "tank4" ]
  && backupNames == [ "backup1" "backup3" "backup4" ]
  && builtins.attrNames disks == [ "ssd" "tank1" "tank2" "tank3" "tank4" ]
  && layout.disko.devices.zpool.backups.mode == "raidz"
  && builtins.attrNames layout.disko.devices.zpool.tank.datasets == [ "archive" ]
  && builtins.attrNames layout.disko.devices.zpool.backups.datasets == [ "data" "data/recovery" ]
  && server.fileSystems."/srv/backup".device == "backups/data"
  && lib.all (name:
    disks.${name}.device == physical.hdds.${name}
    && disks.${name}.content.partitions.crypt.content.name == "${name}-crypt"
    && disks.${name}.content.partitions.crypt.size == physical.mainPartitionSize
    && disks.${name}.content.partitions.crypt.content.content.pool == "tank"
    && lib.hasInfix "${name}-crypt ${disks.${name}.device}-part1 - tpm2-device=auto,tpm2-pcrs=7,tpm2-signature="
      server.environment.etc.crypttab.text
  ) hddNames
  && builtins.attrNames disks.tank2.content.partitions == [ "crypt" ]
  && lib.all (name:
    let partitions = disks.${"tank" + lib.removePrefix "backup" name}.content.partitions;
    in builtins.attrNames partitions == [ name "crypt" ]
      && partitions.${name}.priority > partitions.crypt.priority
      && partitions.${name}.content.name == "${name}-crypt"
      && partitions.${name}.content.content.pool == "backups"
      && lib.hasInfix "${name}-crypt ${physical.backupHdds.${name}}-part2 - tpm2-device=auto,tpm2-pcrs=7,tpm2-signature="
        server.environment.etc.crypttab.text
  ) backupNames
) "Four main sections and three larger-drive tails must form separate RAIDZ1 pools with matching encrypted identities";
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
    lib.hasInfix "-W 0,0,80" smartOptions.${device}
    && lib.hasInfix "-s (S/../.././12|L/../../7/13)" smartOptions.${device}
  ) (builtins.attrValues physical.ssds)
  && lib.hasInfix "-i 194 " smartOptions.${physical.ssds.fast}
  && lib.all (device: !(lib.hasInfix "-i 194" smartOptions.${device}))
    ([ physical.ssds.root ] ++ builtins.attrValues physical.hdds)
  && lib.all (name:
    let options = smartOptions.${physical.hdds.${name}};
        schedule = {
          tank1 = "S/../.././12|L/../../7/14";
          tank2 = "S/../.././12|L/../../7/15";
          tank3 = "S/../.././12|L/../../7/13";
          tank4 = "S/../.././12|L/../../7/16";
        }.${name};
    in lib.hasInfix "-W 0,0,55" options
      && lib.hasInfix "-s (${schedule})" options
  ) hddNames
  && server.services.zfs.autoScrub.enable
  && server.services.zfs.autoScrub.pools == [ "backups" "fast" "tank" ]
) "SMART must schedule daily short tests, weekly long tests, high-temperature-only warnings and pool scrubs";
assert lib.assertMsg (
  capacityPolicy.pools == [ "backups" "fast" "tank" ]
  && capacityPolicy.warning_percent == 75
  && capacityPolicy.critical_percent == 80
  && capacityPolicy.reminder_hours == 24
  && capacityPolicy.key_file == server.sops.secrets.nextcloud-resend.path
  && lib.hasInfix " zfs-capacity /etc/zfs-capacity.json"
    server.systemd.services.zfs-capacity-alerts.serviceConfig.ExecStart
  && server.systemd.services.zfs-capacity-alerts.serviceConfig.StateDirectoryMode == "0700"
  && server.systemd.services.zfs-capacity-alerts.serviceConfig.ProtectSystem == "strict"
  && server.systemd.timers.zfs-capacity-alerts.timerConfig.OnCalendar == "*:0/5"
  && server.systemd.timers.zfs-capacity-alerts.timerConfig.Persistent
) "All pools must have persistent, non-destructive 75/80 percent capacity alerts with five-minute checks";
assert lib.assertMsg (
  lib.hasSuffix "/bin/gtrash put" server.environment.shellAliases.rm
  && lib.all (name:
    let
      service = server.systemd.services."gtrash-prune-${name}";
      timer = server.systemd.timers."gtrash-prune-${name}";
    in service.serviceConfig.User == name
      && lib.hasSuffix "/bin/gtrash prune --day 30 --force" service.serviceConfig.ExecStart
      && service.serviceConfig.ProtectSystem == "full"
      && service.serviceConfig.ProtectHome == false
      && service.serviceConfig.NoNewPrivileges
      && service.serviceConfig.SetLoginEnvironment
      && service.environment.XDG_DATA_HOME == "${server.users.users.${name}.home}/.local/share"
      && !(builtins.elem "zfs-import-tank.service" service.requires)
      && timer.timerConfig.OnCalendar == "daily"
      && timer.timerConfig.Persistent
  ) [ "mirsella" "root" ]
) "Interactive deletion must use gtrash and daily private pruning must retain 30 days without requiring optional pools";
assert lib.assertMsg (
  server.services.restic.backups.fast.repository == "/srv/backup/restic"
  && server.services.restic.backups.fast.initialize
  && server.services.restic.backups.fast.pruneOpts == [
    "--keep-daily 7"
    "--keep-weekly 8"
    "--keep-monthly 6"
  ]
  && server.services.restic.backups.fast.paths == [
    "/srv/data/fast/.zfs/snapshot/restic-backup"
    "/var/lib/nextcloud/data/.zfs/snapshot/restic-backup"
  ]
  && lib.hasInfix "zfs snapshot" server.services.restic.backups.fast.backupPrepareCommand
  && lib.hasInfix "zfs destroy" server.services.restic.backups.fast.backupCleanupCommand
  && server.systemd.timers.restic-check.timerConfig.Persistent
  && lib.all (name: server.services.sanoid.datasets.${name}.autosnap)
    [ "fast/data" "fast/ncdata" "tank/archive" "backups/data" ]
  && lib.all (unit:
    builtins.elem "zfs-import-backups.service" server.systemd.services.${unit}.requires
    && !(builtins.elem "zfs-import-tank.service" server.systemd.services.${unit}.requires)
  ) [ "restic-backups-fast" "restic-check" ]
  && lib.all (dataset: dataset.recursive) (builtins.attrValues server.services.sanoid.datasets)
) "The SSD pool must be backed up nightly by restic from a throwaway snapshot, while HDD datasets keep their reliable sanoid snapshots";
assert lib.assertMsg (
  lib.all (name:
    server.systemd.services.${name}.serviceConfig.UMask == "0077"
    && server.systemd.timers.${name}.timerConfig.Persistent
    && server.systemd.services.${name}.unitConfig.RequiresMountsFor == [ "/srv/backup/recovery" ]
  ) [ "db-backup" "recovery-backup" ]
  && lib.toList server.systemd.timers.recovery-backup.timerConfig.OnCalendar == [ "Sun 23:10" ]
  && lib.toList server.systemd.timers.db-backup.timerConfig.OnCalendar == [ "23:00" ]
  && server.fileSystems."/srv/backup/recovery".device == "backups/data/recovery"
  && layout.disko.devices.zpool.backups.datasets."data/recovery".mountpoint == "/srv/backup/recovery"
  && layout.disko.devices.zpool.backups.datasets."data/recovery".options.mountpoint == "legacy"
  && builtins.elem "nofail" server.fileSystems."/srv/backup/recovery".options
) "Database and recovery bundles must share a private legacy mount without blocking boot on failure";
assert lib.assertMsg (
  lib.toList server.systemd.services.hermes-backup.startAt == [ "23:20" ]
  && builtins.elem "multi-user.target" server.systemd.services.hermes-backup.after
  && server.systemd.services.hermes-backup.unitConfig.RequiresMountsFor == [ "/srv/backup" ]
  && server.systemd.services.hermes-backup.serviceConfig.UMask == "0077"
  && server.systemd.timers.hermes-backup.timerConfig.Persistent
  && server.fileSystems."/srv/backup".device == "backups/data"
  && lib.any (path: lib.hasInfix "util-linux" (toString path)) server.systemd.services.hermes-backup.path
  && builtins.elem pkgs.gzip server.systemd.services.hermes-backup.path
) "Hermes runtime state must be backed up daily to the mounted backups dataset";
assert lib.assertMsg (
  server.systemd.services.caddy.serviceConfig.Restart == "on-failure"
) "The server's web proxy must retry transient startup failures";
assert lib.assertMsg (
  lib.all (key: server.sops.secrets."healthchecks-${key}".key == key)
    [ "db-backup" "recovery-backup" "hermes-backup" "sanoid" "restic-backup" "restic-check" ]
  && lib.all (unit:
    let pings = server.systemd.services.${unit}.unitConfig.OnSuccess or [ ];
    in builtins.length pings == 1 && lib.hasInfix "healthchecks-ping@" (builtins.head pings)
  ) [ "db-backup" "recovery-backup" "hermes-backup" "sanoid" "restic-backups-fast" "restic-check" ]
  && lib.toList server.systemd.timers.restic-backups-fast.timerConfig.OnCalendar == [ "23:50" ]
  && !(server.systemd.timers.restic-backups-fast.unitConfig.X-OnlyManualStart or true)
  && lib.hasInfix "--read-data-subset=10%%" server.systemd.services.restic-check.serviceConfig.ExecStart
  && lib.all (mount: builtins.elem mount server.systemd.services."restic-backups-fast".unitConfig.RequiresMountsFor)
    [ "/srv/backup" "/srv/data/fast" "/var/lib/nextcloud/data" ]
) "Every scheduled backup must ping its healthchecks.io check and restic must back up from mounted sources";
assert lib.assertMsg (
  lib.all (change:
    let updated = (flake.nixosConfigurations.predator.extendModules {
      modules = [ change ];
    }).config;
    in lib.all (unit:
      map toString updated.systemd.services.${unit}.restartTriggers
        != map toString server.systemd.services.${unit}.restartTriggers
    ) [ "hermes-agent" "hermes-backend" ]
  ) [
    { services.hermes-agent.settings.approvals.mode = lib.mkForce "smart"; }
    { services.hermes-agent.documents."AGENTS.md" = lib.mkForce "Restart test instructions"; }
    { services.hermes-agent.hermesHomeFiles."SOUL.md" = lib.mkForce "Restart test personality"; }
    { services.hermes-agent.extraPlugins = lib.mkForce []; }
    { services.hermes-agent.settings.mcp_servers.slack.oauth.timeout = lib.mkForce 601; }
  ]
) "Managed Hermes config, instructions, personality and plugin changes must restart both services";
assert lib.assertMsg (
  let
    slack = server.services.hermes-agent.settings.mcp_servers.slack;
    scopes = lib.splitString " " slack.oauth.scope;
    expectedScopes = [
      "canvases:read" "canvases:write"
      "channels:history" "channels:read" "channels:write"
      "chat:write" "emoji:read" "files:read" "files:write"
      "groups:history" "groups:read" "groups:write"
      "im:history" "im:read" "im:write" "lists:read" "lists:write"
      "mpim:history" "mpim:read" "mpim:write"
      "reactions:read" "reactions:write"
      "search:read.files" "search:read.im" "search:read.mpim"
      "search:read.private" "search:read.public" "search:read.users"
      "users:read" "users:read.email"
    ];
  in slack.url == "https://mcp.slack.com/mcp"
  && slack.auth == "oauth"
  && slack.oauth.client_id == "\${SLACK_MCP_CLIENT_ID}"
  && slack.oauth.client_secret == "\${SLACK_MCP_CLIENT_SECRET}"
  && slack.oauth.token_endpoint_auth_method == "client_secret_post"
  && slack.oauth.redirect_host == "localhost"
  && slack.oauth.redirect_port == 27890
  && lib.sort builtins.lessThan scopes == lib.sort builtins.lessThan expectedScopes
  && !lib.hasInfix "admin:" slack.oauth.scope
  && !((server.services.hermes-agent.settings.platforms or {}) ? slack)
) "Hermes Slack must grant all official MCP user scopes with secret references and no bot gateway";
assert lib.assertMsg (
  server.services.hermes-agent.user == "mirsella"
  && server.services.hermes-agent.group == "hermes-private"
  && server.users.groups.hermes-private.members == [ "mirsella" ]
  && server.users.users.mirsella.linger
  && !server.services.hermes-agent.createUser
   && server.services.hermes-agent.settings.skills.external_dirs == [ "/home/mirsella/.agents/skills" "/home/mirsella/.codex/skills" ]
   && server.services.hermes-agent.settings.skills.disabled == [ "personal-accounts" ]
   && !(server.services.hermes-agent.hermesHomeFiles ? "skills/personal-accounts/SKILL.md")
   && server.services.hermes-agent.settings.browser == { backend = "browserbase"; cloud_provider = "camofox"; }
   && server.services.hermes-agent.settings.agent.disabled_toolsets == [ "browser-unsupported" ]
   && server.services.hermes-agent.settings.plugins.enabled == [ "browser-policy" ]
   && server.services.hermes-agent.settings.model.provider == "openai-codex"
    && server.services.hermes-agent.settings.model.default == "gpt-6.1-sol"
    && server.services.hermes-agent.settings.agent.reasoning_effort == "high"
    && server.services.hermes-agent.settings.agent.reasoning_overrides."gpt-6.1-sol" == "high"
   && server.services.hermes-agent.settings.fallback_providers == []
   && server.services.hermes-agent.settings.custom_providers == []
   && server.services.hermes-agent.settings.providers.openai-codex.enabled
   && lib.all (name: !server.services.hermes-agent.settings.providers.${name}.enabled)
     [ "openrouter" "openai" "openai-api" "opencode-go" "chatgpt" "opencode" "opencode-zen" "zen" "custom" "auto" "anthropic" ]
    && lib.all (slot: slot.provider == "openai-codex" && slot.model == "gpt-6.1-sol" && slot.reasoning_effort == "high" && slot.api_key == "" && slot.base_url == "")
     (builtins.attrValues (builtins.removeAttrs server.services.hermes-agent.settings.auxiliary [ "openrouter_model" ]))
  && lib.elem "hermes-agent-setup" server.system.activationScripts.hermes-settings.deps
  && lib.hasInfix "runuser -u mirsella -g hermes-private" server.system.activationScripts.hermes-settings.text
  && lib.hasInfix "hermes-configure" server.system.activationScripts.hermes-settings.text
  && lib.all (unit: lib.hasInfix "hermes refresh-context" server.systemd.services.${unit}.preStart)
    [ "hermes-agent" "hermes-backend" ]
  && server.services.hermes-agent.environment.HERMES_RUNTIME_DIR == toString pkgs.emptyDirectory
  && server.services.hermes-agent.environment.HERMES_MEDIA_ALLOW_DIRS == "/var/lib/hermes/workspace"
  && !server.services.hermes-agent.settings.security.allow_lazy_installs
  && server.systemd.services.hermes-agent.environment.HOME == "/home/mirsella"
  && server.systemd.services.hermes-agent.environment.HERMES_MANAGED == "false"
  && server.services.hermes-agent.environment.HERMES_CODEX_BASE_URL == "http://127.0.0.1:17321/sleev/hermes/codex"
   && !(server.services.hermes-agent.environment ? OPENCODE_ZEN_BASE_URL)
   && !(server.services.hermes-agent.environment ? OPENAI_BASE_URL)
   && !(server.services.hermes-agent.environment ? OPENCODE_GO_BASE_URL)
   && lib.all (name: lib.elem name server.systemd.services.hermes-agent.serviceConfig.UnsetEnvironment && lib.elem name server.systemd.services.hermes-backend.serviceConfig.UnsetEnvironment)
     [ "OPENROUTER_API_KEY" "OPENAI_API_KEY" "OPENCODE_GO_API_KEY" "OPENCODE_ZEN_API_KEY" "DEEPINFRA_API_KEY" "MOONSHOT_API_KEY" ]
  && server.services.hermes-agent.environment.TELEGRAM_ALLOW_ALL_USERS == "false"
  && map lib.getName server.services.hermes-agent.extraPlugins == [ "hermes-browser-policy" ]
  && server.services.hermes-agent.package.hermesVenv.drvPath
    == flake.inputs.hermes-agent.packages.x86_64-linux.messaging.hermesVenv.drvPath
  && server.services.hermes-agent.package.hermesWeb.drvPath
    != flake.inputs.hermes-agent.packages.x86_64-linux.messaging.hermesWeb.drvPath
  && lib.hasInfix "base: \"/hermes/\"" server.services.hermes-agent.package.hermesWeb.postPatch
  && server.services.hermes-agent.documents ? "AGENTS.md"
  && !(server.services.hermes-agent.settings ? hooks)
  && server.services.hermes-agent.settings.approvals.mode == "off"
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
  && server.systemd.services.hermes-browser-control.serviceConfig.Type == "notify"
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
    [ "handle /browser/*" "import private_ui" "header_up X-Hermes-Viewer-Key" "header_up -Authorization" ]
  && lib.hasInfix "https://mirsella.mooo.com{uri}" server.services.caddy.virtualHosts."http://:80".extraConfig
  && lib.all (text: lib.hasInfix text server.services.caddy.extraConfig)
    [ "(private_ui)" "basic_auth" "{$HERMES_VIEWER_PASSWORD_HASH}" "Cache-Control no-store" ]
  && lib.all (text: lib.hasInfix text server.services.caddy.virtualHosts."mirsella.mooo.com".extraConfig)
    [ "Content-Security-Policy" "style-src 'sha256-" "script-src 'sha256-" ]
  && builtins.length (lib.splitString "import private_ui"
    server.services.caddy.virtualHosts."mirsella.mooo.com".extraConfig) >= 4
  && server.systemd.services.caddy.serviceConfig.EnvironmentFile == [ server.sops.secrets.hermes_caddy_env.path ]
  && server.sops.secrets.hermes_caddy_env.restartUnits == [ "caddy.service" ]
  && server.services.caddy.virtualHosts."http://:80".extraConfig
    == "redir https://mirsella.mooo.com{uri} 308\n"
) "Hermes must use the owner account with the Nix-owned approval gate, while its browser stays private";
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
