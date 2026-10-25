{
  config,
  isNixOS,
  lib,
  pkgs,
  ...
}:
let
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
  imports = [ ./electron-theme.nix ./desktop-mcp.nix ./adb-wireless.nix ./codexbar.nix ];

  programs.git.settings.commit.gpgsign = true;
  home.packages = [
    pkgs.kache
    pkgs.idle-watchdog
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
  # Cargo uses this stable path so cargo-installed kache cannot shadow its daemon.
  home.file.".local/bin/kache" = {
    source = lib.getExe pkgs.kache;
    force = true;
  };
  home.file.".local/bin/opencode-idle-watchdog" = {
    source = "${pkgs.idle-watchdog}/bin/opencode-idle-watchdog";
    force = true;
  };
  home.file.".local/bin/t3-idle-watchdog" = {
    source = "${pkgs.idle-watchdog}/bin/t3-idle-watchdog";
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
  home.file.".local/bin/plasma-memory-summary" = {
    source = "${pkgs.host-tools}/bin/plasma-memory-summary";
    force = true;
  };
  home.file.".local/bin/plasma-power-summary" = {
    source = "${pkgs.host-tools}/bin/plasma-power-summary";
    force = true;
  };
  xdg.configFile =
    lib.genAttrs
      (map (directory: "opencode/${directory}") [
        "plugins"
        "tui-plugins"
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
        rclone_conf = {
          sopsFile = ../../secrets/webdav.yaml;
          path = "${config.home.homeDirectory}/.config/rclone/rclone.conf";
          mode = "0600";
        };
      };
  };

  systemd.user.services.openchamber = {
    Unit = {
      Description = "OpenChamber web server";
      After = [
        "network.target"
        "sops-nix.service"
      ];
      Wants = [ "sops-nix.service" ];
      PartOf = [ "default.target" ];
    };
    Service = {
      Type = "simple";
      WorkingDirectory = "%h";
      EnvironmentFile = [
        config.sops.secrets.openchamber_server.path
      ];
      Environment = servicePath;
      # Attach-to-14096 pins from the retired shared server; let OpenChamber manage its own OpenCode.
      UnsetEnvironment = [
        "OPENCODE_HOST"
        "OPENCODE_SKIP_START"
      ];
      ExecStart = "${lib.getExe pkgs.openchamber} serve --host 127.0.0.1 --port 14097 --foreground";
      Restart = "on-failure";
      RestartSec = "2s";
    };
    Install.WantedBy = [ "default.target" ];
  };

  # KIO browses this on demand without exposing an offline cloud mount to local trash scans.
  xdg.dataFile."remoteview/nextcloud.desktop".text = ''
    [Desktop Entry]
    Type=Link
    Name=Nextcloud
    Icon=folder-cloud
    URL=webdavs://mirsella@mirsella.mooo.com/nextcloud/remote.php/dav/files/mirsella/
  '';

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

}
