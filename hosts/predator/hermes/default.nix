{ config, lib, pkgs, inputs, ... }:
let
  cfg = config.predator.hermes;
  state = "/var/lib/hermes";
  browserState = "/var/lib/camofox";
  controlState = "/var/lib/hermes-browser-control";
  tools = pkgs.buildEnv {
    name = "hermes-workspace-tools";
    paths = with pkgs; [ bashInteractive coreutils findutils gnugrep gnused gawk git ripgrep curl jq python3 nodejs_22 cacert ];
  };
  package = inputs.hermes-agent.packages.${pkgs.stdenv.hostPlatform.system}.messaging;
  upstreamCommon = import "${inputs.hermes-agent}/nix/moduleCommon.nix" { inherit lib; };
  gatewayEnvironment = upstreamCommon.mkEnvScript {
    inherit pkgs;
    inherit (config.services.hermes-agent) environment;
  };
  controllerConfig = pkgs.writeText "hermes-browser-control.json" (builtins.toJSON {
    listen = "127.0.0.1:9378";
    backend = "http://127.0.0.1:9377";
    websocket = "ws://127.0.0.1:6080/websockify";
    origin = "https://mirsella.mooo.com";
    state = "${controlState}/lifecycle.json";
    novnc = "${pkgs.novnc}/share/novnc";
  });
  aux = lib.genAttrs [ "vision" "compression" "skills_hub" "approval" "review" "mcp" "title_generation" "memory_query_rewrite" "tts_tags" "voice_chat" "triage" "kanban_decomposer" "profile_describer" "goal_judge" "curator" "monitor" "background_review" ] (_: {
    provider = "opencode-go"; model = "muse-spark-1.3-contributor";
  });
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
    CPUAccounting = true;
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
        meta skuid { "hermes-agent", "camofox" } ip daddr { 127.0.0.53, 192.168.1.1 } udp dport 53 accept
        meta skuid { "hermes-agent", "camofox" } ip daddr { 127.0.0.53, 192.168.1.1 } tcp dport 53 accept
        meta skuid "hermes-agent" ip daddr 127.0.0.1 tcp dport 9378 accept
        meta skuid "camofox" ip daddr 127.0.0.1 tcp dport 5900 accept
        meta skuid { "hermes-agent", "camofox" } ip daddr { 0.0.0.0/8, 10.0.0.0/8, 100.64.0.0/10, 127.0.0.0/8, 169.254.0.0/16, 172.16.0.0/12, 192.168.0.0/16, 198.18.0.0/15, 224.0.0.0/4, 240.0.0.0/4 } reject
        meta skuid { "hermes-agent", "camofox" } ip6 daddr { ::/128, ::1/128, fc00::/7, fe80::/10, ff00::/8 } reject
      }
    }
  '';
in {
  options.predator.hermes = {
    enable = lib.mkEnableOption "the isolated Hermes/Camofox deployment";
    messaging = lib.mkOption { type = lib.types.bool; default = false; description = "Enable the gateway after Telegram secrets and the numeric owner allowlist are provisioned"; };
  };
  config = lib.mkIf cfg.enable {
    sops.secrets = lib.genAttrs [ "hermes_gateway_env" "hermes_browser_env" "hermes_control_env" "hermes_caddy_env" ] (name: {
      sopsFile = ../../../secrets/hermes.yaml;
      restartUnits = if name == "hermes_caddy_env" then [ "caddy.service" ] else [ "camofox-browser.service" "hermes-browser-control.service" ] ++ lib.optional cfg.messaging "hermes-agent.service";
    } // lib.optionalAttrs (name == "hermes_gateway_env") {
      owner = config.services.hermes-agent.user;
      group = config.services.hermes-agent.group;
      mode = "0400";
    });
    users.groups.camofox = {};
    users.groups.hermes-browser-control = {};
    users.users.camofox = { isSystemUser = true; group = "camofox"; home = browserState; };
    users.users.hermes-browser-control = { isSystemUser = true; group = "hermes-browser-control"; home = controlState; };
    systemd.tmpfiles.rules = [
      "d ${browserState} 0750 camofox camofox -"
      "d ${browserState}/profiles 0700 camofox camofox -"
      "d ${browserState}/cache 0700 camofox camofox -"
      "d ${controlState} 0700 hermes-browser-control hermes-browser-control -"
      # Only temporary downloaded files are shared, never browser profiles/tmp.
      # setgid uses the existing agent group; no extra service account/group.
      "d /var/lib/camofox-downloads 2750 camofox hermes-agent -"
      "d ${state}/workspace/downloads 0750 hermes-agent hermes-agent -"
    ];
    services.hermes-agent = {
      enable = true;
      inherit package;
      user = "hermes-agent";
      group = "hermes-agent";
      createUser = true;
      stateDir = state;
      workingDirectory = "${state}/workspace";
      addToSystemPackages = false;
      environmentFiles = [ config.sops.secrets.hermes_gateway_env.path ];
      documents."AGENTS.md" = builtins.readFile ./agent-AGENTS.md;
      extraPackages = [ tools ];
      environment = {
        CAMOFOX_URL = "http://127.0.0.1:9378";
        CAMOFOX_USER_ID = "home-browser";
        CAMOFOX_SESSION_KEY = "home-browser";
        CAMOFOX_ADOPT_EXISTING_TAB = "true";
        TELEGRAM_ALLOW_ALL_USERS = "false";
        GATEWAY_ALLOW_ALL_USERS = "false";
      };
      settings = {
        model = { provider = "opencode-go"; default = "muse-spark-1.3-contributor"; };
        fallback_providers = [];
        auth.adopt_external_logins = false;
        # Use upstream agent/tool/approval/cron defaults. These are only the
        # deployment's account, browser and private-messaging settings.
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
    };
    systemd.services.hermes-agent = {
      wantedBy = lib.mkForce (lib.optional cfg.messaging "multi-user.target");
      requires = [ "hermes-network-isolation.service" "hermes-browser-control.service" "sops-install-secrets.service" ];
      after = [ "hermes-network-isolation.service" "hermes-browser-control.service" "sops-install-secrets.service" ];
      # Predator installs SOPS through systemd, after native Nix activation.
      # Reuse upstream's env renderer after installation, including on restart.
      preStart = lib.mkBefore ''
        test -r ${lib.escapeShellArg config.sops.secrets.hermes_gateway_env.path}
        ${gatewayEnvironment} ${state}/.hermes/.env 0640 ${lib.escapeShellArgs config.services.hermes-agent.environmentFiles}
      '';
      serviceConfig = hardening // {
        # Mask personal homes while exposing the user bus for native cron scopes.
        ProtectHome = lib.mkForce "tmpfs";
        UMask = lib.mkForce "0077";
        Restart = lib.mkForce "on-failure";
        RestartSec = lib.mkForce "5s";
        BindReadOnlyPaths = [ "/run/user" "/var/lib/camofox-downloads:${state}/workspace/downloads" ];
      };
    };
    systemd.services.hermes-network-isolation = {
      description = "Deny Hermes/browser access to private networks and raw browser ports";
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
      path = with pkgs; [ xorg.xorgserver x11vnc python3Packages.websockify procps coreutils findutils gnugrep gnused fontconfig bash ];
      environment = {
        HOME = browserState;
        CAMOFOX_BIND_HOST = "127.0.0.1";
        CAMOFOX_PORT = "9377";
        CAMOUFOX_EXECUTABLE = "${pkgs.camoufox}/lib/camoufox/camoufox-bin";
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
    environment.systemPackages = [ pkgs.host-tools pkgs.sops pkgs.ssh-to-age ];
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
  };
}
