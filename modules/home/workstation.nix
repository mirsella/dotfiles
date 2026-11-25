{
  config,
  isNixOS,
  lib,
  pkgs,
  ...
}:
{
  imports = [
    ./arch-package-cache.nix
    ./electron-theme.nix
    ./nasa-wallpaper.nix
    ./plasma.nix
    ./desktop-mcp.nix
    ./adb-wireless.nix
    ./codexbar.nix
    ./claude-credit-router.nix
    ./kdeconnect-remote-commands.nix
    ./reverse-engineering.nix
    ./hex.nix
    ./kache-observations.nix
  ];

  programs.git.settings.commit.gpgsign = true;
  home.packages = [
    pkgs.kache
    pkgs.idle-watchdog
    pkgs.context7-account-broker
  ]
  ++ lib.optionals isNixOS [
    pkgs.binaryen
    pkgs.gnumake
    pkgs.jdk21
    pkgs.julia
    pkgs.kotlin
    pkgs.mermaid-cli
    pkgs.ninja
    pkgs.trunk
    pkgs.wasm-pack
  ];
  programs.opencode.enable = false;
  # Cargo uses this stable path so cargo-installed kache cannot shadow its daemon.
  home.file.".local/bin/kache" = {
    source = lib.getExe pkgs.kache;
    force = true;
  };
  home.file.".local/bin/kache-engine" = {
    source = "${pkgs.kache}/bin/kache-engine";
    force = true;
  };
  # Keep Cargo-installed frontends from replacing the managed daemon.
  home.file.".local/share/cargo/bin/kache" = {
    source = lib.getExe pkgs.kache;
    force = true;
  };
  home.file.".local/share/cargo/bin/kache-engine" = {
    source = "${pkgs.kache}/bin/kache-engine";
    force = true;
  };
  home.file.".local/share/cargo/bin/kache-clang-cl" = {
    source = "${pkgs.kache}/bin/kache-clang-cl";
    force = true;
  };
  home.file.".local/share/cargo/bin/kache-cl" = {
    source = "${pkgs.kache}/bin/kache-cl";
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
        "chunks"
      ])
      (path: {
        source = "${pkgs.opencode-extensions}/share/${path}";
        force = true;
      })
    // {
      "opencode/inkling-compaction.path".text = "${pkgs.opencode-extensions}/share/opencode/inkling-compaction";

      # KMail, calendars and contacts are unused. Mask D-Bus activation too so
      # an installed PIM dependency cannot start Akonadi's private database.
      "systemd/user/akonadi_control.service" = {
        source = config.lib.file.mkOutOfStoreSymlink "/dev/null";
        force = true;
      };

      # Temporary: preserve a stack trace if the intermittent logout hang returns.
      # TODO: remove after diagnosis; SIGKILL otherwise discards the blocked threads.
      "systemd/user/plasma-plasmashell.service.d/90-shutdown-debug.conf".text = ''
        [Service]
        TimeoutStopFailureMode=abort
        TimeoutAbortSec=10s
      '';

      "kache/config.toml" = {
        force = true;
        text = ''
          [cache]
          local_max_size = "150GiB"
          cache_executables = false
          explain_miss = true
          record_sessions = true
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

  # nixpkgs ships ONLYOFFICE 9.4 with LanguageTool built in; Arch's 10.0 does not.
  # The desktop app only scans real directories under data/sdkjs-plugins, so link
  # the plugin's files individually rather than the directory.
  xdg.dataFile."onlyoffice/desktopeditors/data/sdkjs-plugins/{7CDB02C9-A0BF-4B56-9A1A-71C860B8498F}" =
    lib.mkIf (!isNixOS)
      {
        source = "${pkgs.onlyoffice-languagetool}/share/onlyoffice-languagetool";
        recursive = true;
        force = true;
      };

  # KIO browses this on demand without exposing an offline cloud mount to local trash scans.
  xdg.dataFile."remoteview/nextcloud.desktop".text = ''
    [Desktop Entry]
    Type=Link
    Name=Nextcloud
    Icon=folder-cloud
    URL=webdavs://mirsella@mirsella.mooo.com/nextcloud/remote.php/dav/files/mirsella/
  '';

  systemd.user.services.disk-alerts = lib.mkIf (!isNixOS) {
    Unit.Description = "Warn about low disk and Btrfs allocation space";
    Service = {
      Type = "oneshot";
      ExecStart = "${pkgs.host-tools}/bin/host-tools disk-alerts /";
      StateDirectory = "disk-alerts";
      Environment = [
        "PATH=/usr/bin:/bin"
        "DBUS_SESSION_BUS_ADDRESS=unix:path=%t/bus"
      ];
    };
  };

  systemd.user.timers.disk-alerts = lib.mkIf (!isNixOS) {
    Unit.Description = "Check disk and Btrfs allocation space";
    Timer = {
      OnBootSec = "2min";
      OnUnitInactiveSec = "5min";
      AccuracySec = "30s";
    };
    Install.WantedBy = [ "timers.target" ];
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
      Environment = [ "KACHE_LOG=kache=info" ];
    };
    Install.WantedBy = [ "default.target" ];
  };

}
