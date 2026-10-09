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
  blockedInferenceEnvironment = [ "OPENROUTER_API_KEY" "OPENAI_API_KEY" "OPENCODE_ZEN_API_KEY" "OPENCODE_API_KEY" "DEEPINFRA_API_KEY" "MOONSHOT_API_KEY" "ANTHROPIC_API_KEY" "ANTHROPIC_TOKEN" "CLAUDE_CODE_OAUTH_TOKEN" ];
  state = "/var/lib/hermes";
  browserState = "/var/lib/camofox";
  controlState = "/var/lib/hermes-browser-control";
  accountTools = pkgs.runCommand "hermes-account-tools" { nativeBuildInputs = [ pkgs.makeWrapper ]; } ''
    mkdir -p "$out/bin"
    ${lib.concatStringsSep "\n" (lib.mapAttrsToList (name: executable: ''
      makeWrapper ${executable} "$out/bin/${name}" \
        ${lib.concatStringsSep " \\\n        " (lib.mapAttrsToList (variable: value:
          "--set ${variable} ${lib.escapeShellArg value}"
        ) (builtins.removeAttrs ownerEnvironment [ "HOME" ]))} \
        ${lib.optionalString (name == "pass-cli") ''
          --add-flags '--shared ${ownerRuntime}/hermes-pass.lock ${lib.getExe pkgs.proton-pass-cli}' \
          --set PROTON_PASS_LINUX_KEYRING dbus
        ''}
    '') {
      pass-cli = lib.getExe' pkgs.util-linux "flock";
      protonmail-cli = lib.getExe pkgs.protonmail-cli;
      gh = lib.getExe pkgs.gh;
      secret-tool = lib.getExe' pkgs.libsecret "secret-tool";
    })}
  '';
  tools = pkgs.buildEnv {
    name = "hermes-workspace-tools";
    paths = with pkgs; [ bashInteractive coreutils findutils gnugrep gnused gawk git ripgrep curl jq python3 nodejs_22 cacert accountTools openssh chezmoi sudo nix sleev ];
  };
  upstreamPackage = inputs.hermes-agent.packages.${pkgs.stdenv.hostPlatform.system}.messaging;
  # Vite's lazy preloads must use the authenticated mount, not site-root assets.
  dashboard = upstreamPackage.hermesWeb.overrideAttrs (old: {
    postPatch = (old.postPatch or "") + ''
      substituteInPlace web/vite.config.ts \
        --replace-fail 'export default defineConfig({' 'export default defineConfig({ base: "/hermes/",'
    '';
  });
  # Upstream injects frontend dependencies through callPackage. Keep its native
  # assembly until it exposes a direct frontend parameter.
  package = upstreamPackage.override {
    callPackage = file: arguments:
      if toString file == "${inputs.hermes-agent}/nix/web.nix" then dashboard
      else pkgs.callPackage file arguments;
  };
  upstreamCommon = import "${inputs.hermes-agent}/nix/moduleCommon.nix" { inherit lib; };
  gatewayEnvironment = upstreamCommon.mkEnvScript {
    inherit pkgs;
    inherit (config.services.hermes-agent) environment;
  };
  agentUnits = lib.optional cfg.messaging "hermes-agent.service";
  secretUnits = {
    hermes_gateway_env = [ "hermes-backend.service" ] ++ agentUnits;
    hermes_browser_env = [ "camofox-browser.service" "hermes-browser-control.service" ] ++ agentUnits;
    hermes_control_env = [ "hermes-browser-control.service" ] ++ agentUnits;
    hermes_caddy_env = [ "caddy.service" ];
    hermes_keyring_password = [ "hermes-secret-service.service" "hermes-agent.service" "hermes-backend.service" ];
    hermes_pass_token = [ "hermes-pass-login.service" ];
  };
  controllerConfig = pkgs.writeText "hermes-browser-control.json" (builtins.toJSON {
    listen = "127.0.0.1:9378";
    backend = "http://127.0.0.1:9377";
    websocket = "ws://127.0.0.1:6080/websockify";
    origin = "https://mirsella.mooo.com";
    state = "${controlState}/lifecycle.json";
    novnc = "${pkgs.novnc}/share/webapps/novnc";
  });
  # Native provider policy is checked before alias expansion. Disable both
  # canonical providers and their pinned aliases, including generic endpoints.
  disabledProviders = lib.unique (
    builtins.attrNames (builtins.readDir "${inputs.hermes-agent}/plugins/model-providers") ++ [
      "auto" "moa" "openai" "openai-api" "chatgpt" "chatgpt-codex"
      "opencode" "opencode_zen" "zen" "glm" "z-ai" "z.ai" "zhipu"
      "x-ai" "x.ai" "grok" "xai-oauth" "grok-oauth" "x-ai-oauth" "xai-grok-oauth"
      "nim" "nvidia-nim" "build-nvidia" "nemotron" "kimi-for-coding" "kimi" "kimi-coding-cn" "moonshot"
      "step" "stepfun-coding-plan" "minimax-cn" "minimax-china" "minimax_cn"
      "claude" "claude-code" "github-copilot" "github" "github-copilot-acp"
      "vercel" "aigateway" "vercel-ai-gateway" "kilo" "kilo-code" "kilo-gateway" "deep-seek"
      "dashscope" "aliyun" "qwen" "alibaba-cloud" "alibaba_coding" "alibaba-coding" "alibaba_coding_plan"
      "hf" "hugging-face" "huggingface-hub" "novita-ai" "novitaai" "mimo" "xiaomi-mimo"
      "tencent" "tokenhub" "tencent-cloud" "tencentmaas" "tencent-tokenplan" "tokenplan" "tencent-lkeap"
      "aws" "aws-bedrock" "amazon-bedrock" "amazon" "arcee-ai" "arceeai" "gmi-cloud" "gmicloud"
      "fireworks-ai" "fw" "solar" "actual-computer" "actualcomputer" "aci"
      "nebius" "nebius-tokenfactory" "nebius-tf" "token-factory" "tokenfactory"
      "lm-studio" "lm_studio" "ollama" "local" "vllm" "llamacpp" "llama.cpp" "llama-cpp"
    ]
  );
  aux = lib.genAttrs [ "vision" "compression" "skills_hub" "approval" "review" "mcp" "title_generation" "memory_query_rewrite" "tts_audio_tags" "voice_chat" "triage_specifier" "kanban_decomposer" "profile_describer" "goal_judge" "curator" "monitor" "background_review" "moa_reference" "moa_aggregator" ] (_: {
    provider = "opencode-go";
    model = "muse-spark-1.3-contributor";
    base_url = "";
    api_key = "";
  });
  defaults = {
    terminal = { backend = "local"; cwd = "${state}/workspace"; };
    browser = { backend = "browserbase"; cloud_provider = "camofox"; };
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
      --set HERMES_HOME ${state}/.hermes --set HERMES_MANAGED false \
      ${lib.concatMapStringsSep " \\\n      " (name: "--unset ${name}") blockedInferenceEnvironment}
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
  ownerService = {
    # Dashboard chat and terminal tools have the same owner access as Telegram.
    NoNewPrivileges = lib.mkForce false;
    ProtectSystem = lib.mkForce false;
    ProtectHome = lib.mkForce false;
    PrivateTmp = lib.mkForce false;
    EnvironmentFile = [ "${ownerHome}/.config/environment.d/55-secrets.conf" ];
    # Keep personal application credentials, but do not expose unrelated
    # inference credentials from the owner's general environment to Hermes.
    UnsetEnvironment = blockedInferenceEnvironment;
    UMask = lib.mkForce "0077";
    Restart = lib.mkForce "on-failure";
    RestartSec = lib.mkForce "5s";
    MemoryAccounting = true;
    BindReadOnlyPaths = [ "/var/lib/camofox-downloads:${state}/workspace/downloads" ];
  };
  renderEnvironment = ''
    test -r ${lib.escapeShellArg config.sops.secrets.hermes_gateway_env.path}
    ${gatewayEnvironment} ${state}/.hermes/.env 0600 ${lib.escapeShellArgs config.services.hermes-agent.environmentFiles}
  '';
  isolationRules = pkgs.writeText "predator-hermes-network.nft" ''
    destroy table inet predator_hermes
    table inet predator_hermes {
      chain output {
        type filter hook output priority -10; policy accept;
        # Loopback is reachable by other local services. Gate raw transports
        # and Caddy's configuration API by the initiating socket's UID too.
        ip daddr 127.0.0.0/8 tcp dport { 9377, 6080 } meta skuid != { "root", "hermes-browser-control" } reject
        ip6 daddr ::1 tcp dport { 9377, 6080 } meta skuid != { "root", "hermes-browser-control" } reject
        ip daddr 127.0.0.0/8 tcp dport { 9378, 2019, 9119 } meta skuid != { "root", "${owner}", "caddy" } reject
        ip6 daddr ::1 tcp dport { 9378, 2019, 9119 } meta skuid != { "root", "${owner}", "caddy" } reject
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
    } // lib.optionalAttrs (builtins.elem name [ "hermes_gateway_env" "hermes_keyring_password" "hermes_pass_token" ]) {
      inherit owner;
      group = ownerGroup;
      mode = "0400";
    }) secretUnits;
    users.users.${owner}.uid = 1000;
    home-manager.users.${owner}.home.packages = [ (lib.hiPrio accountTools) ];
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
      backend = {
        mode = "dashboard";
        host = "127.0.0.1";
        port = 9119;
        extraArgs = [ "--skip-build" ];
      };
      environmentFiles = [ config.sops.secrets.hermes_gateway_env.path ];
      documents."AGENTS.md" = builtins.readFile ./agent-AGENTS.md;
      hermesHomeFiles."skills/personal-accounts/SKILL.md" = builtins.readFile ./personal-accounts.md;
      extraPackages = [ tools ];
      environment = {
        HERMES_MANAGED = "false";
        OPENCODE_GO_BASE_URL = "http://127.0.0.1:17321/sleev/hermes/opencode-go";
        CAMOFOX_URL = "http://127.0.0.1:9378";
        CAMOFOX_USER_ID = "home-browser";
        CAMOFOX_SESSION_KEY = "home-browser";
        CAMOFOX_ADOPT_EXISTING_TAB = "true";
        TELEGRAM_ALLOW_ALL_USERS = "false";
        GATEWAY_ALLOW_ALL_USERS = "false";
      };
      # Inference policy is Nix-owned; other runtime settings remain editable.
      settings = {
        model = { provider = "opencode-go"; default = "muse-spark-1.3-contributor"; base_url = ""; api_key = ""; };
        auxiliary = aux // { openrouter_model = ""; };
        fallback_providers = [];
        custom_providers = [];
        providers = lib.genAttrs disabledProviders (_: { enabled = false; }) // { opencode-go.enabled = true; };
        auth.adopt_external_logins = false;
        skills.external_dirs = [ "${ownerHome}/.agents/skills" "${ownerHome}/.codex/skills" ];
      };
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
        ExecStart = "${pkgs.gnome-keyring}/bin/gnome-keyring-daemon --foreground --unlock --components=secrets";
        StandardInput = "file:${config.sops.secrets.hermes_keyring_password.path}";
        Restart = "on-failure";
        RestartSec = "5s";
        UMask = "0077";
      };
    };
    # PAT tokens last a year, but their cached sessions last only two hours.
    # Authenticate at boot and hourly through the vendor's native env API.
    systemd.services.hermes-pass-login = {
      description = "Renew the headless Proton Pass session";
      wantedBy = [ "multi-user.target" ];
      requires = [ "hermes-secret-service.service" "sops-install-secrets.service" ];
      after = [ "hermes-secret-service.service" "sops-install-secrets.service" "network-online.target" ];
      wants = [ "network-online.target" ];
      environment = ownerEnvironment // { PROTON_PASS_LINUX_KEYRING = "dbus"; };
      startAt = "hourly";
      serviceConfig = {
        Type = "oneshot";
        User = owner;
        Group = ownerGroup;
        ExecStart = "${pkgs.host-tools}/bin/host-tools pass-login --client ${pkgs.proton-pass-cli}/bin/pass-cli --token-file ${config.sops.secrets.hermes_pass_token.path}";
        Restart = "on-failure";
        RestartSec = "1min";
        TimeoutStartSec = "90s";
        UMask = "0077";
      };
    };
    systemd.timers.hermes-pass-login.timerConfig.Persistent = true;
    systemd.services.hermes-agent = {
      wantedBy = lib.mkForce (lib.optional cfg.messaging "multi-user.target");
      requires = [ "hermes-browser-control.service" "sops-install-secrets.service" "sleev-gateway.service" "hermes-secret-service.service" ];
      after = [ "hermes-browser-control.service" "sops-install-secrets.service" "sleev-gateway.service" "hermes-secret-service.service" "hermes-pass-login.service" ];
      wants = [ "hermes-pass-login.service" ];
      environment = ownerEnvironment // {
        HOME = lib.mkForce ownerHome;
        HERMES_MANAGED = lib.mkForce "false";
        PATH = lib.mkForce "/run/wrappers/bin:${ownerHome}/.local/share/cargo/bin:/etc/profiles/per-user/${owner}/bin:/run/current-system/sw/bin:${lib.makeBinPath [ tools package pkgs.systemd ]}";
        PROTON_PASS_LINUX_KEYRING = "dbus";
      };
      # Predator installs SOPS through systemd, after native Nix activation.
      # Reuse upstream's env renderer after installation, including on restart.
      preStart = lib.mkBefore renderEnvironment;
      serviceConfig = ownerService;
    };
    systemd.services.hermes-backend = {
      requires = [ "hermes-network-isolation.service" "sops-install-secrets.service" "sleev-gateway.service" "hermes-secret-service.service" ];
      after = [ "hermes-network-isolation.service" "sops-install-secrets.service" "sleev-gateway.service" "hermes-secret-service.service" "hermes-agent.service" ];
      wants = [ "hermes-agent.service" "hermes-pass-login.service" ];
      environment = lib.mapAttrs (_: value: lib.mkForce value) config.systemd.services.hermes-agent.environment;
      preStart = lib.mkBefore renderEnvironment;
      serviceConfig = ownerService;
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
    systemd.services.hermes-backup = {
      description = "Back up Hermes runtime state to Tank";
      startAt = "23:20";
      path = [ pkgs.gnutar pkgs.util-linux ];
      unitConfig.RequiresMountsFor = [ "/srv/backup" ];
      serviceConfig = {
        Type = "oneshot";
        ExecStart = "${pkgs.host-tools}/bin/host-tools hermes backup";
        TimeoutStartSec = "30min";
        UMask = "0077";
      };
    };
    systemd.timers.hermes-backup.timerConfig.Persistent = true;
    systemd.services.caddy.serviceConfig.EnvironmentFile = [ config.sops.secrets.hermes_caddy_env.path ];
    # After alone does not pull in the online target during early boot.
    systemd.services.caddy.wants = [ "network-online.target" ];
    environment.systemPackages = [ pkgs.host-tools pkgs.sops pkgs.ssh-to-age pkgs.sleev ownerCli ];
    services.caddy.virtualHosts."mirsella.mooo.com".extraConfig = lib.mkBefore ''
      redir /hermes /hermes/ 308
      handle_path /hermes/* {
        route {
          basic_auth {
            mirsella {$HERMES_VIEWER_PASSWORD_HASH}
          }
          @foreignOrigin expression `{http.request.header.Origin} != "" && {http.request.header.Origin} != "https://mirsella.mooo.com"`
          respond @foreignOrigin "Same-origin access required" 403
          header {
            Cache-Control "no-store"
            Referrer-Policy "no-referrer"
            X-Content-Type-Options "nosniff"
            X-Frame-Options "DENY"
          }
          reverse_proxy 127.0.0.1:9119 {
            header_up Host {upstream_hostport}
            header_up X-Forwarded-Prefix /hermes
            # Validate the real Origin above, then present the loopback
            # authority expected by the native dashboard's Host/WS guard.
            header_up Origin http://127.0.0.1:9119
            header_up -Authorization
          }
        }
      }
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
      @hermesViewer path /browser /browser/* /hermes /hermes/*
      redir @hermesViewer https://mirsella.mooo.com{uri} 308
    '';
  };
}
