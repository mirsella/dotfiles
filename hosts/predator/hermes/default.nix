{ config, lib, pkgs, inputs, ... }:
let
  cfg = config.predator.hermes;
  owner = "mirsella";
  # Upstream state directories are group-writable. Keep private state and
  # downloads out of the shared "users" group.
  ownerGroup = "hermes-private";
  ownerHome = config.users.users.${owner}.home;
  ownerRuntime = "/run/user/${toString config.users.users.${owner}.uid}";
  ownerEnvironment = {
    HOME = ownerHome;
    XDG_RUNTIME_DIR = ownerRuntime;
    DBUS_SESSION_BUS_ADDRESS = "unix:path=${ownerRuntime}/bus";
  };
  state = "/var/lib/hermes";
  browserState = "/var/lib/camofox";
  controlState = "/var/lib/hermes-browser-control";
  tools = pkgs.buildEnv {
    name = "hermes-workspace-tools";
    paths = with pkgs; [ bashInteractive coreutils findutils gnugrep gnused gawk git ripgrep curl jq python3 nodejs_22 cacert gh proton-pass-cli protonmail-cli openssh chezmoi sudo nix sleev ];
  };
  package = inputs.hermes-agent.packages.${pkgs.stdenv.hostPlatform.system}.messaging;
  upstreamCommon = import "${inputs.hermes-agent}/nix/moduleCommon.nix" { inherit lib; };
  gatewayEnvironment = upstreamCommon.mkEnvScript {
    inherit pkgs;
    inherit (config.services.hermes-agent) environment;
  };
  agentUnits = lib.optional cfg.messaging "hermes-agent.service";
  secretUnits = {
    hermes_gateway_env = agentUnits;
    hermes_browser_env = [ "camofox-browser.service" "hermes-browser-control.service" ] ++ agentUnits;
    hermes_control_env = [ "hermes-browser-control.service" ] ++ agentUnits;
    hermes_caddy_env = [ "caddy.service" ];
  };
  controllerConfig = pkgs.writeText "hermes-browser-control.json" (builtins.toJSON {
    listen = "127.0.0.1:9378";
    backend = "http://127.0.0.1:9377";
    websocket = "ws://127.0.0.1:6080/websockify";
    origin = "https://mirsella.mooo.com";
    state = "${controlState}/lifecycle.json";
    novnc = "${pkgs.novnc}/share/webapps/novnc";
  });
  aux = lib.genAttrs [ "vision" "compression" "skills_hub" "approval" "review" "mcp" "title_generation" "memory_query_rewrite" "tts_tags" "voice_chat" "triage" "kanban_decomposer" "profile_describer" "goal_judge" "curator" "monitor" "background_review" ] (_: {
    provider = "opencode-go"; model = "muse-spark-1.3-contributor";
  });
  defaults = {
    model = { provider = "opencode-go"; default = "muse-spark-1.3-contributor"; };
    fallback_providers = [];
    auth.adopt_external_logins = false;
    terminal = { backend = "local"; cwd = "${state}/workspace"; };
    browser = { backend = "browserbase"; cloud_provider = "camofox"; };
    auxiliary = aux;
    gateway = { allow_all_users = false; unauthorized_dm_behavior = "ignore"; };
    platforms.telegram.extra = {
      group_allow_from = [];
      group_allow_admin_from = [];
      allowed_chats = [ "932980505" ];
      guest_mode = false;
    };
  };
  ownerCli = pkgs.runCommand "hermes-owner-cli" { nativeBuildInputs = [ pkgs.makeWrapper ]; } ''
    mkdir -p "$out/bin"
    makeWrapper ${package}/bin/hermes "$out/bin/hermes" \
      --set HERMES_HOME ${state}/.hermes --set HERMES_MANAGED false
  '';
  hardening = {
    NoNewPrivileges = true;
    ProtectSystem = "strict";
    ProtectHome = true;
    PrivateTmp = true;
    ProtectKernelTunables = true;
    ProtectKernelModules = true;
    ProtectControlGroups = true;
    RestrictSUIDSGID = true;
    LockPersonality = true;
    CapabilityBoundingSet = "";
    UMask = "0077";
    MemoryAccounting = true;
    Restart = "on-failure";
    RestartSec = "5s";
    TimeoutStopSec = "90s";
  };
  isolationRules = pkgs.writeText "predator-hermes-network.nft" ''
    destroy table inet predator_hermes
    table inet predator_hermes {
      chain output {
        type filter hook output priority -10; policy accept;
        # Loopback is reachable by other local services. Gate raw transports
        # and Caddy's configuration API by the initiating socket's UID too.
        ip daddr 127.0.0.0/8 tcp dport { 9377, 6080 } meta skuid != { "root", "hermes-browser-control" } reject
        ip6 daddr ::1 tcp dport { 9377, 6080 } meta skuid != { "root", "hermes-browser-control" } reject
        ip daddr 127.0.0.0/8 tcp dport { 9378, 2019 } meta skuid != { "root", "${owner}", "caddy" } reject
        ip6 daddr ::1 tcp dport { 9378, 2019 } meta skuid != { "root", "${owner}", "caddy" } reject
        ip daddr 127.0.0.0/8 tcp dport 5900 meta skuid != { "root", "camofox" } reject
        ip6 daddr ::1 tcp dport 5900 meta skuid != { "root", "camofox" } reject
        # Let private servers answer approved clients; only new outbound
        # connections are subject to the destination restrictions below.
        meta skuid "camofox" ct state established,related accept
        meta skuid "camofox" ip daddr { 127.0.0.53, 192.168.1.1 } udp dport 53 accept
        meta skuid "camofox" ip daddr { 127.0.0.53, 192.168.1.1 } tcp dport 53 accept
        meta skuid "camofox" ip daddr 127.0.0.1 tcp dport 5900 accept
        meta skuid "camofox" ip daddr { 0.0.0.0/8, 10.0.0.0/8, 100.64.0.0/10, 127.0.0.0/8, 169.254.0.0/16, 172.16.0.0/12, 192.168.0.0/16, 198.18.0.0/15, 224.0.0.0/4, 240.0.0.0/4 } reject
        meta skuid "camofox" ip6 daddr { ::/128, ::1/128, fc00::/7, fe80::/10, ff00::/8 } reject
      }
    }
  '';
in {
  options.predator.hermes = {
    enable = lib.mkEnableOption "Hermes with the shared private browser";
    messaging = lib.mkOption { type = lib.types.bool; default = false; description = "Enable the gateway after Telegram secrets and the numeric owner allowlist are provisioned"; };
  };
  config = lib.mkIf cfg.enable {
    # Restart each secret's consumer and its dependent services. Model or
    # Telegram credential changes do not need to close the shared browser.
    sops.secrets = lib.mapAttrs (name: restartUnits: {
      sopsFile = ../../../secrets/hermes.yaml;
      inherit restartUnits;
    } // lib.optionalAttrs (name == "hermes_gateway_env") {
      owner = config.services.hermes-agent.user;
      group = config.services.hermes-agent.group;
      mode = "0400";
    }) secretUnits // { hermes_keyring_password = {
      sopsFile = ../../../secrets/hermes.yaml;
      owner = owner;
      group = ownerGroup;
      mode = "0400";
      restartUnits = [ "hermes-secret-service.service" "hermes-agent.service" ];
    }; };
    users.users.${owner}.uid = 1000;
    users.groups.camofox = {};
    users.groups.hermes-private.members = [ owner ];
    users.groups.hermes-browser-control = {};
    users.users.camofox = { isSystemUser = true; group = "camofox"; home = browserState; };
    users.users.hermes-browser-control = { isSystemUser = true; group = "hermes-browser-control"; home = controlState; };
    systemd.tmpfiles.rules = [
      "d ${ownerHome}/.config/sleev 0700 ${owner} ${ownerGroup} -"
      "d ${ownerHome}/.local/share/sleev 0700 ${owner} ${ownerGroup} -"
      "d ${ownerHome}/.local/share/keyrings 0700 ${owner} ${ownerGroup} -"
      "d ${browserState} 0750 camofox camofox -"
      "d ${browserState}/profiles 0700 camofox camofox -"
      "d ${browserState}/cache 0700 camofox camofox -"
      "d ${controlState} 0700 hermes-browser-control hermes-browser-control -"
      # Only temporary downloaded files are shared, never browser profiles/tmp.
      # The private group shares downloads only with the owner.
      "d /var/lib/camofox-downloads 2750 camofox ${ownerGroup} -"
      "d ${state}/workspace/downloads 0750 ${owner} ${ownerGroup} -"
    ];
    services.hermes-agent = {
      enable = true;
      inherit package;
      user = owner;
      group = ownerGroup;
      createUser = false;
      stateDir = state;
      workingDirectory = "${state}/workspace";
      addToSystemPackages = false;
      environmentFiles = [ config.sops.secrets.hermes_gateway_env.path ];
      documents."AGENTS.md" = builtins.readFile ./agent-AGENTS.md;
      hermesHomeFiles."skills/personal-accounts/SKILL.md" = builtins.readFile ./personal-accounts.md;
      extraPackages = [ tools ];
      environment = {
        HERMES_MANAGED = "false";
        OPENCODE_ZEN_BASE_URL = "http://127.0.0.1:17321/sleev/hermes/opencode";
        OPENCODE_GO_BASE_URL = "http://127.0.0.1:17321/sleev/hermes/opencode-go";
        OPENAI_BASE_URL = "http://127.0.0.1:17321/sleev/hermes/openai";
        HERMES_CODEX_BASE_URL = "http://127.0.0.1:17321/sleev/hermes/codex";
        CAMOFOX_URL = "http://127.0.0.1:9378";
        CAMOFOX_USER_ID = "home-browser";
        CAMOFOX_SESSION_KEY = "home-browser";
        CAMOFOX_ADOPT_EXISTING_TAB = "true";
        TELEGRAM_ALLOW_ALL_USERS = "false";
        GATEWAY_ALLOW_ALL_USERS = "false";
      };
      # Seed missing settings once; runtime changes survive subsequent rebuilds.
      settings = {};
    };
    system.activationScripts.hermes-settings = lib.stringAfter [ "hermes-agent-setup" ] ''
      ${pkgs.util-linux}/bin/runuser -u ${owner} -g ${ownerGroup} -- \
        ${pkgs.host-tools}/bin/host-tools hermes-configure \
        --config ${state}/.hermes/config.yaml \
        --defaults ${pkgs.writers.writeJSON "hermes-defaults.json" defaults}
    '';
    systemd.services.sleev-gateway = {
      description = "Sleev inference gateway";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      environment = { HOME = ownerHome; SLEEV_CLI_VERSION = pkgs.sleev.version; };
      serviceConfig = {
        User = owner;
        Group = ownerGroup;
        ExecStart = "${pkgs.sleev}/bin/sleeve-gateway --config ${ownerHome}/.config/sleev/gateway.json --db-path ${ownerHome}/.local/share/sleev/sleeve.sqlite --host 127.0.0.1 --port 17321";
        Restart = "on-failure";
        RestartSec = "5s";
        UMask = "0077";
        MemoryAccounting = true;
      };
    };
    systemd.services.hermes-secret-service = {
      description = "Unlocked personal Secret Service for Proton account tools";
      wantedBy = [ "multi-user.target" ];
      requires = [ "user@${toString config.users.users.${owner}.uid}.service" "sops-install-secrets.service" ];
      after = [ "user@${toString config.users.users.${owner}.uid}.service" "sops-install-secrets.service" ];
      environment = ownerEnvironment;
      serviceConfig = {
        User = owner;
        Group = ownerGroup;
        ExecStart = "${pkgs.host-tools}/bin/host-tools secret-service --daemon ${pkgs.gnome-keyring}/bin/gnome-keyring-daemon --password-file ${config.sops.secrets.hermes_keyring_password.path}";
        Restart = "on-failure";
        RestartSec = "5s";
        UMask = "0077";
      };
    };
    systemd.services.hermes-agent = {
      wantedBy = lib.mkForce (lib.optional cfg.messaging "multi-user.target");
      requires = [ "hermes-browser-control.service" "sops-install-secrets.service" "sleev-gateway.service" "hermes-secret-service.service" ];
      after = [ "hermes-browser-control.service" "sops-install-secrets.service" "sleev-gateway.service" "hermes-secret-service.service" ];
      environment = ownerEnvironment // {
        HOME = lib.mkForce ownerHome;
        HERMES_MANAGED = lib.mkForce "false";
        PATH = lib.mkForce "/run/wrappers/bin:${ownerHome}/.local/share/cargo/bin:/etc/profiles/per-user/${owner}/bin:/run/current-system/sw/bin:${lib.makeBinPath [ tools package pkgs.systemd ]}";
        PROTON_PASS_LINUX_KEYRING = "dbus";
      };
      # Predator installs SOPS through systemd, after native Nix activation.
      # Reuse upstream's env renderer after installation, including on restart.
      preStart = lib.mkBefore ''
        test -r ${lib.escapeShellArg config.sops.secrets.hermes_gateway_env.path}
        ${gatewayEnvironment} ${state}/.hermes/.env 0600 ${lib.escapeShellArgs config.services.hermes-agent.environmentFiles}
      '';
      serviceConfig = {
        # Run with the owner's ordinary access, including passwordless sudo.
        NoNewPrivileges = lib.mkForce false;
        ProtectSystem = lib.mkForce false;
        ProtectHome = lib.mkForce false;
        PrivateTmp = lib.mkForce false;
        EnvironmentFile = [ "${ownerHome}/.config/environment.d/55-secrets.conf" ];
        UMask = lib.mkForce "0077";
        Restart = lib.mkForce "on-failure";
        RestartSec = lib.mkForce "5s";
        MemoryAccounting = true;
        BindReadOnlyPaths = [ "/var/lib/camofox-downloads:${state}/workspace/downloads" ];
      };
    };
    systemd.services.hermes-network-isolation = {
      description = "Deny browser access to private networks and raw browser ports";
      wantedBy = [ "multi-user.target" ];
      before = [ "hermes-agent.service" "camofox-browser.service" "hermes-browser-control.service" ];
      after = [ "systemd-sysusers.service" ];
      serviceConfig = { Type = "oneshot"; RemainAfterExit = true; ExecStart = "${pkgs.nftables}/bin/nft -f ${isolationRules}"; };
    };
    systemd.services.camofox-browser = {
      description = "Private lazy Camofox browser and virtual display";
      wantedBy = [ "multi-user.target" ];
      requires = [ "hermes-network-isolation.service" ];
      after = [ "hermes-network-isolation.service" "sops-install-secrets.service" ];
      path = with pkgs; [ xorg.xorgserver x11vnc python3Packages.websockify procps coreutils findutils gnugrep gnused gawk fontconfig bash which ];
      environment = {
        HOME = browserState;
        CAMOFOX_BIND_HOST = "127.0.0.1";
        CAMOFOX_PORT = "9377";
        CAMOUFOX_EXECUTABLE = "${pkgs.camoufox}/lib/camoufox/camoufox-bin";
        CAMOUFOX_INSTALL_DIR = "${pkgs.camoufox}/lib/camoufox";
        CAMOFOX_DOWNLOADS_DIR = "/var/lib/camofox-downloads";
        XDG_CACHE_HOME = "${browserState}/cache";
        CAMOFOX_PREWARM = "false";
        SESSION_TIMEOUT_MS = "0";
        BROWSER_IDLE_TIMEOUT_MS = "0";
        TAB_INACTIVITY_MS = "0";
        CAMOFOX_CRASH_REPORT_ENABLED = "false";
        CAMOFOX_DISABLE_DEFAULT_ADDONS = "true";
        CAMOFOX_LOCALE = "en-US";
        CAMOFOX_TIMEZONE = "Europe/Paris";
        CAMOFOX_INTERACTIVE = "novnc";
        ENABLE_VNC = "true";
        VNC_RESOLUTION = "1600x900";
        VNC_BIND = "127.0.0.1";
        VNC_RFB_BIND = "127.0.0.1";
        VNC_PORT = "5900";
        NOVNC_PORT = "6080";
        MAX_SESSIONS = "1";
        MAX_TABS_GLOBAL = "2";
        MAX_TABS_PER_SESSION = "2";
      };
      serviceConfig = hardening // {
        User = "camofox"; Group = "camofox";
        EnvironmentFile = config.sops.secrets.hermes_browser_env.path;
        ExecStart = lib.getExe pkgs.camofox-browser;
        WorkingDirectory = browserState;
        ReadWritePaths = [ browserState "/var/lib/camofox-downloads" ];
        KillMode = "mixed";
      };
    };
    systemd.services.hermes-browser-control = {
      description = "Private browser viewer, on-demand startup and idle cleanup";
      wantedBy = [ "multi-user.target" ];
      requires = [ "camofox-browser.service" "hermes-network-isolation.service" ];
      after = [ "camofox-browser.service" "hermes-network-isolation.service" "sops-install-secrets.service" ];
      serviceConfig = hardening // {
        User = "hermes-browser-control"; Group = "hermes-browser-control";
        # Drain the bounded startup/tool request, then checkpoint and stop.
        TimeoutStopSec = 300;
        EnvironmentFile = config.sops.secrets.hermes_control_env.path;
        ExecStart = "${pkgs.host-tools}/bin/host-tools browser-control --config ${controllerConfig}";
        ReadWritePaths = [ controlState ];
      };
    };
    systemd.services.caddy.serviceConfig.EnvironmentFile = [ config.sops.secrets.hermes_caddy_env.path ];
    environment.systemPackages = [ pkgs.host-tools pkgs.sops pkgs.ssh-to-age pkgs.sleev ownerCli ];
    services.caddy.virtualHosts."mirsella.mooo.com".extraConfig = lib.mkBefore ''
      redir /browser /browser/ 308
      handle /browser/* {
        basic_auth {
          mirsella {$HERMES_VIEWER_PASSWORD_HASH}
        }
        header {
          Cache-Control "no-store"
          Referrer-Policy "no-referrer"
          X-Content-Type-Options "nosniff"
          X-Frame-Options "DENY"
        }
        reverse_proxy 127.0.0.1:9378 {
          header_up X-Hermes-Viewer-Key {$HERMES_VIEWER_KEY}
          header_up -Authorization
        }
      }
    '';
    services.caddy.virtualHosts."http://:80".extraConfig = lib.mkBefore ''
      @hermesViewer path /browser /browser/*
      redir @hermesViewer https://mirsella.mooo.com{uri} 308
    '';
  };
}
