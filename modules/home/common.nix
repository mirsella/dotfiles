{ lib, config, osConfig ? null, pkgs, gitSigningKey, isNixOS, ... }:
let
  secrets = if isNixOS then osConfig.sops.secrets else config.sops.secrets;
  exe =
    pkg: bin:
    if isNixOS then "${pkgs.${pkg}}/bin/${bin}" else "/usr/bin/${bin}";
in
{
  home = {
    username = "mirsella";
    homeDirectory = "/home/mirsella";
    stateVersion = "26.05";
    activation.rustupNightly = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
      export CARGO_HOME="$HOME/.local/share/cargo" RUSTUP_HOME="$HOME/.local/share/rustup"
      export PATH="${lib.optionalString isNixOS "${pkgs.rustup}/bin:"}$PATH"
      if ! rustup toolchain list 2>/dev/null | grep -q '^nightly'; then
        run rustup toolchain install nightly --profile minimal --component rust-src
      fi
      run rustup default nightly
    '';
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
      recursive = [
        "environment.d"
        "opencode"
      ];
      appOwned = lib.optionals (!isNixOS) [ "plasma-workspace" ];
      dirs = lib.removeAttrs all appOwned;
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
    lib.mapAttrs' (
      n: _:
      lib.nameValuePair n (
        { source = ./files/config + "/${n}"; }
        // lib.optionalAttrs (lib.elem n recursive) { recursive = true; }
      )
    ) dirs
    // builtins.listToAttrs (
      map (f: {
        name = "nushell/${f}";
        value.source = ./files/nushell/${f};
      }) nushell
    )
    // {
      "nvim" = {
        source = ./files/nvim;
        recursive = true;
      };
      "starship.toml".source = ./files/starship.toml;
      "atuin/config.toml".source = ./files/atuin/config.toml;
    };

  systemd.user.services = {
    opencode = {
      Unit = {
        Description = "OpenCode server";
        After = [ "network.target" ] ++ lib.optional (!isNixOS) "sops-nix.service";
        PartOf = [ "default.target" ];
      };
      Service = {
        Type = "simple";
        WorkingDirectory = "%h";
        EnvironmentFile = [
          secrets.telegram_env.path
          secrets.opencode_server.path
        ];
        Environment = "PATH=%h/.local/share/cargo/bin:%h/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/bin";
        ExecStart = "${exe "opencode" "opencode"} serve --hostname 127.0.0.1 --port 14096";
        Restart = "on-failure";
        RestartSec = "2s";
      };
      Install.WantedBy = [ "default.target" ];
    };
    lspmux = {
      Unit.Description = "Language server multiplexer server";
      Service = {
        Type = "simple";
        ExecStart = "${exe "lspmux" "lspmux"} server";
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
        ExecStart = "${exe "rclone" "rclone"} mount --vfs-cache-mode full gdrive: %h/Documents/gdrive";
        ExecStop = "${exe "fuse" "fusermount"} -u %h/Documents/gdrive";
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
        ExecStart = "${exe "rclone" "rclone"} mount --vfs-cache-mode full gdrive-voxride: %h/Documents/gdrive-voxride";
        ExecStop = "${exe "fuse" "fusermount"} -u %h/Documents/gdrive-voxride";
        Restart = "always";
        RestartSec = 3;
      };
    };
  };

  programs = {
    git = {
      enable = true;
      package = if isNixOS then pkgs.git else null;
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
      } // lib.optionalAttrs (gitSigningKey != null) {
        commit.gpgsign = true;
        user.signingkey = gitSigningKey;
      };
    };
    ssh = {
      enable = true;
      enableDefaultConfig = false;
      package = if isNixOS then pkgs.openssh else null;
      settings = {
        rpi = {
          HostName = "192.168.1.166";
          User = "mirsella";
        };
        laptop = {
          HostName = "192.168.1.61";
          User = "mirsella";
        };
        main = {
          HostName = "192.168.1.131";
          User = "mirsella";
        };
      };
    };
  };
}
