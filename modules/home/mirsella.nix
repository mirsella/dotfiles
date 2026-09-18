{ config, lib, pkgs, ... }:
{
  sops.secrets =
    let
      userSecret = path: {
        sopsFile = ../../secrets/services.yaml;
        owner = config.home.username;
        inherit path;
      };
    in
    {
      telegram_env = userSecret "${config.home.homeDirectory}/.config/telegram.env";
      opencode_server = userSecret "${config.home.homeDirectory}/.config/opencode/server.env";
      openchamber_server = userSecret "${config.home.homeDirectory}/.config/openchamber/server.env";
      env_secrets = userSecret "${config.home.homeDirectory}/.config/environment.d/55-secrets.conf";
      stuff_config = userSecret "${config.home.homeDirectory}/.config/stuff/config.toml";
      context7_accounts = userSecret "${config.home.homeDirectory}/.config/context7-account-broker/accounts.json";
      gdrive_gcp = userSecret "${config.home.homeDirectory}/.config/google-drive-mcp/gcp-oauth.keys.json";
      gdrive_tokens = userSecret "${config.home.homeDirectory}/.config/google-drive-mcp/tokens.json";
    };

  home = {
    username = "mirsella";
    homeDirectory = "/home/mirsella";
    stateVersion = "26.05";
    packages = with pkgs; [
      atuin
      bat
      delta
      difftastic
      fd
      fzf
      gh
      gtrash
      jujutsu
      lazygit
      lazyjj
      lsd
      mergiraf
      neovim
      nushell
      ripgrep
      starship
      zoxide
    ];
    file = {
      ".config/git/ignore".source = ./files/git/ignore;
      ".config/git/attributes".source = ./files/git/attributes;
      ".local/bin".source = ./files/local/bin;
      ".agents".source = ./files/agents;
      ".codex".source = ./files/codex;
    };
  };

  xdg.configFile =
    let
      all = builtins.readDir ./files/config;
      merged = [
        "environment.d"
        "google-drive-mcp"
        "openchamber"
        "opencode"
        "stuff"
        "context7-account-broker"
      ];
      plain = lib.removeAttrs all merged;
      nushell = [
        "alias.nu"
        "completions.nu"
        "config.nu"
        "env.nu"
        "functions.nu"
        "laptop.nu"
        "notif.nu"
        "plugins.nu"
      ];
    in
    lib.mapAttrs' (n: _: lib.nameValuePair n { source = ./files/config + "/${n}"; }) plain
    // builtins.listToAttrs (
      map (n: {
        name = n;
        value = {
          source = ./files/config + "/${n}";
          recursive = true;
        };
      }) merged
    )
    // builtins.listToAttrs (
      map (f: {
        name = "nushell/${f}";
        value.source = ./files/nushell/${f};
      }) nushell
    )
    // {
      "nvim".source = ./files/nvim;
      "starship.toml".source = ./files/starship.toml;
      "atuin/config.toml".source = ./files/atuin/config.toml;
    };

  systemd.user.services = {
    opencode = {
      Unit = {
        Description = "OpenCode server";
        After = [ "network.target" ];
        PartOf = [ "default.target" ];
      };
      Service = {
        Type = "simple";
        WorkingDirectory = "%h";
        EnvironmentFile = [
          config.sops.secrets.telegram_env.path
          config.sops.secrets.opencode_server.path
        ];
        Environment = "PATH=%h/.local/share/cargo/bin:%h/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/bin";
        ExecStart = "${pkgs.opencode}/bin/opencode serve --hostname 127.0.0.1 --port 14096";
        Restart = "on-failure";
        RestartSec = "2s";
      };
      Install.WantedBy = [ "default.target" ];
    };
    lspmux = {
      Unit.Description = "Language server multiplexer server";
      Service = {
        Type = "simple";
        ExecStart = "${pkgs.lspmux}/bin/lspmux server";
        Restart = "on-failure";
        RestartSec = "5s";
      };
      Install.WantedBy = [ "default.target" ];
    };
    rclone-gdrive = {
      Unit = {
        Description = "mount google drive with rclone";
        After = [ "network.target" ];
      };
      Service = {
        Type = "simple";
        ExecStartPre = "-mkdir -p %h/Documents/gdrive";
        ExecStart = "${pkgs.rclone}/bin/rclone mount --vfs-cache-mode full gdrive: %h/Documents/gdrive";
        ExecStop = "${pkgs.fuse}/bin/fusermount -u %h/Documents/gdrive";
        Restart = "always";
        RestartSec = 3;
      };
    };
    rclone-gdrive-voxride = {
      Unit = {
        Description = "mount Voxride Google Drive with rclone";
        After = [ "network.target" ];
      };
      Service = {
        Type = "simple";
        ExecStartPre = "-mkdir -p %h/Documents/gdrive-voxride";
        ExecStart = "${pkgs.rclone}/bin/rclone mount --vfs-cache-mode full gdrive-voxride: %h/Documents/gdrive-voxride";
        ExecStop = "${pkgs.fuse}/bin/fusermount -u %h/Documents/gdrive-voxride";
        Restart = "always";
        RestartSec = 3;
      };
    };
  };

  programs = {
    git = {
      enable = true;
      settings = {
      user.name = "mirsella";
      user.email = "mirsella@protonmail.com";
      init.defaultBranch = "main";
      pull.rebase = true;
      push.autoSetupRemote = true;
      core = {
        excludesfile = "~/.config/git/ignore";
        attributesfile = "~/.config/git/attributes";
        editor = "nvim";
        pager = "delta";
      };
      filter.lfs = {
        required = true;
        clean = "git-lfs clean -- %f";
        smudge = "git-lfs smudge -- %f";
        process = "git-lfs filter-process";
      };
      merge = {
        conflictStyle = "zdiff3";
        tool = "diffview";
      };
      mergetool = {
        prompt = false;
        keepBackup = false;
      };
      "mergetool \"diffview\"".cmd = ''nvim -n -c "DiffEditor $left $right $output"'';
      "merge \"mergiraf\"" = {
        name = "mergiraf";
        driver = "mergiraf merge --timeout 30000 --git %O %A %B -s %S -x %X -y %Y -p %P -l %L";
      };
      diff.external = "difft";
      delta = {
        navigate = true;
        line-numbers = true;
        side-by-side = true;
      };
      interactive.diffFilter = "delta --color-only";
      };
    };
    ssh = {
      enable = true;
      enableDefaultConfig = false;
      matchBlocks = {
        rpi = {
          hostname = "192.168.1.166";
          user = "mirsella";
        };
        laptop = {
          hostname = "192.168.1.61";
          user = "mirsella";
        };
        main = {
          hostname = "192.168.1.131";
          user = "mirsella";
        };
      };
    };
  };
}
