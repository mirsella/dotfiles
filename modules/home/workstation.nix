{
  config,
  isNixOS,
  lib,
  pkgs,
  ...
}:
let
  bin = package: name: if isNixOS then lib.getExe' package name else "/usr/bin/${name}";
  servicePath =
    "PATH=%h/.local/share/cargo/bin:%h/.local/bin:"
    + (
      if isNixOS then
        "%h/.nix-profile/bin:/etc/profiles/per-user/%u/bin:/run/current-system/sw/bin"
      else
        "%h/.nix-profile/bin:/usr/local/sbin:/usr/local/bin:/usr/bin"
    );
in
{
  imports = [ ./electron-theme.nix ];

  programs.git.settings.commit.gpgsign = true;
  home.packages = [
    pkgs.kache
    pkgs.opencode-idle-watchdog
    pkgs.rea
    pkgs.ghidra
    pkgs.context7-account-broker
    pkgs.openchamber
  ]
  ++ lib.optionals isNixOS [
    pkgs.binaryen
    pkgs.gnumake
    pkgs.ninja
    pkgs.trunk
    pkgs.wasm-pack
  ];
  home.file.".config/opencode/skills/reverse-engineer-anything" = {
    source = "${pkgs.rea}/lib/node_modules/rea-agents/skills/reverse-engineer-anything";
    recursive = true;
  };

  programs.opencode.enable = false;
  home.file.".local/bin/opencode-idle-watchdog" = {
    source = lib.getExe pkgs.opencode-idle-watchdog;
    force = true;
  };
  home.file.".local/bin/context7-account-broker" = {
    source = lib.getExe pkgs.context7-account-broker;
    force = true;
  };
  home.file.".local/bin/composio-mcp-headers" = {
    source = "${pkgs.host-tools}/bin/composio-mcp-headers";
    force = true;
  };
  home.file.".local/bin/t3-cache-warmer" = {
    source = lib.getExe pkgs.t3-cache-warmer;
    force = true;
  };
  xdg.configFile =
    lib.genAttrs
      (map (directory: "opencode/${directory}") [
        "plugins"
        "chunks"
      ])
      (path: {
        source = "${pkgs.opencode-extensions}/share/${path}";
        force = true;
      })
    // {
      "kache/config.toml" = {
        force = true;
        text = ''
          [cache]
          local_max_size = "150GiB"
        '';
      };
    }
    //
      lib.genAttrs
        [
          "systemd/user/opencode.service.d/override.conf"
          "systemd/user/app-org.wezfurlong.wezterm@.service.d/override.conf"
          "systemd/user/app-rio\\x2dcargo@.service.d/override.conf"
        ]
        (_: {
          source = ./oom-survival.conf;
          force = true;
        });

  sops = {
    age.sshKeyPaths = [ "${config.home.homeDirectory}/.ssh/id_ed25519" ];
    defaultSopsFile = ../../secrets/services.yaml;
    secrets =
      builtins.mapAttrs
        (_: path: {
          path = "${config.home.homeDirectory}/.config/${path}";
        })
        (
          (import ../user-secrets.nix)
          // {
            telegram_env = "telegram.env";
            openchamber_server = "openchamber/server.env";
          }
        )
      // {
        davfs2_secrets = {
          sopsFile = ../../secrets/webdav.yaml;
          path = "${config.home.homeDirectory}/.davfs2/secrets";
          mode = "0600";
        };
        rclone_conf = {
          sopsFile = ../../secrets/webdav.yaml;
          path = "${config.home.homeDirectory}/.config/rclone/rclone.conf";
          mode = "0600";
        };
      };
  };

  systemd.user.services.opencode = {
    Unit = {
      Description = "OpenCode server";
      After = [
        "network.target"
        "sops-nix.service"
      ];
      PartOf = [ "default.target" ];
      X-SwitchMethod = "keep-old";
    };
    Service = {
      Type = "simple";
      WorkingDirectory = "%h";
      EnvironmentFile = [
        config.sops.secrets.telegram_env.path
      ];
      Environment = servicePath;
      ExecStart = "%h/.local/bin/opencode serve --service --hostname 127.0.0.1 --port 14096";
      Restart = "on-failure";
      RestartSec = "2s";
    };
    Install.WantedBy = [ "default.target" ];
  };

  systemd.user.services.openchamber = {
    Unit = {
      Description = "OpenChamber web server";
      After = [
        "network.target"
        "opencode.service"
        "sops-nix.service"
      ];
      Wants = [
        "opencode.service"
        "sops-nix.service"
      ];
      PartOf = [
        "default.target"
        "opencode.service"
      ];
    };
    Service = {
      Type = "simple";
      WorkingDirectory = "%h";
      EnvironmentFile = [
        config.sops.secrets.openchamber_server.path
      ];
      Environment = servicePath;
      ExecStart = "${lib.getExe pkgs.host-tools} opencode-client-env ${config.xdg.configHome}/opencode/service.json -- ${lib.getExe pkgs.openchamber} serve --host 127.0.0.1 --port 14097 --foreground";
      Restart = "on-failure";
      RestartSec = "2s";
    };
    Install.WantedBy = [ "default.target" ];
  };

  systemd.user.services.rclone-nextcloud =
    let
      mountPoint = "%h/Documents/Nextcloud";
    in
    {
      Unit = {
        Description = "Nextcloud WebDAV mount";
        After = [ "sops-nix.service" ];
        # Home Manager restarts sops-nix on activation; keep mounted files available.
        Wants = [ "sops-nix.service" ];
      };
      Install.WantedBy = [ "default.target" ];
      Service = {
        Type = "notify";
        ExecStartPre = "${bin pkgs.coreutils "mkdir"} -p ${mountPoint}";
        ExecStart = "${bin pkgs.rclone "rclone"} mount nextcloud: ${mountPoint} --vfs-cache-mode writes --poll-interval 0";
        SuccessExitStatus = "143";
        Restart = "always";
        RestartSec = 10;
      };
    };

  # The daemon loads the key on startup; wait for sops-nix to install it.
  systemd.user.services.atuin-daemon = lib.mkIf config.programs.atuin.daemon.enable {
    Unit.Wants = [ "sops-nix.service" ];
    Unit.After = [ "sops-nix.service" ];
  };

  systemd.user.services.kache = {
    Unit.Description = "kache build cache daemon";
    Service = {
      Type = "simple";
      ExecStart = "${lib.getExe' pkgs.kache "kache"} daemon run";
      Restart = "on-failure";
      RestartSec = "5s";
      Environment = "KACHE_LOG=kache=info";
    };
    Install.WantedBy = [ "default.target" ];
  };

  # Sends a one-word keepalive to active T3 Code threads before their prompt
  # cache TTL expires. Reads ~/.config/t3-cache-warmer/config.json (chezmoi)
  # and keeps its credential in ~/.local/state/t3-cache-warmer (mode 600).
  # Sign in once per machine with `t3-cache-warmer login`.
  systemd.user.services.t3-cache-warmer = {
    Unit = {
      Description = "T3 Code prompt-cache warmer";
      After = [ "network.target" ];
      PartOf = [ "default.target" ];
    };
    Service = {
      Type = "simple";
      Environment = servicePath;
      ExecStart = "${lib.getExe pkgs.t3-cache-warmer} run";
      Restart = "always";
      RestartSec = "60s";
    };
    Install.WantedBy = [ "default.target" ];
  };

}
