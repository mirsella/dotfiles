{ lib, config, pkgs, isNixOS, ... }:
let
  lspmuxBin =
    if isNixOS then "${pkgs.lspmux}/bin/lspmux"
    else "${config.home.homeDirectory}/.local/share/cargo/bin/lspmux";

  userSecrets = import ../user-secrets.nix;

  # Arch's pacman owns the atuin binary; this shim only needs a version that
  # satisfies Home Manager's daemon feature gates (>= 18.13.0).
  atuinPkg =
    if isNixOS then
      pkgs.atuin
    else
      pkgs.runCommand "atuin" {
        version = "18.23.0";
        meta.mainProgram = "atuin";
      } ''
        mkdir -p $out/bin
        ln -s /usr/bin/atuin $out/bin/atuin
      '';
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
  };

  # On Arch, activation only installs units; running instances are never
  # started, restarted, or stopped, and take over on next login instead.
  systemd.user.startServices = isNixOS;

  # Root GC cannot prune Arch's Home Manager generations in the user's XDG state.
  nix.gc = {
    automatic = !isNixOS;
    dates = "daily";
    randomizedDelaySec = "1h";
    options = "--delete-older-than 7d";
  };
  systemd.user.services.nix-gc = lib.mkIf (!isNixOS) {
    # Pacman owns Nix on Arch; use that client rather than pulling in another one.
    Service.ExecStart = lib.mkForce "/usr/bin/nix-collect-garbage ${config.nix.gc.options}";
  };

  systemd.user.services.lspmux = {
    Unit.Description = "Language server multiplexer server";
    Service = {
      Type = "simple";
      ExecStart = "${lspmuxBin} server";
      Restart = "on-failure";
      RestartSec = "5s";
    };
    Install.WantedBy = [ "default.target" ];
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
      };
    };
    ssh = {
      enable = true;
      enableDefaultConfig = false;
      package = if isNixOS then pkgs.openssh else null;
      settings = {
        rpi = {
          HostName = "mirsella.mooo.com";
          Port = 222;
          User = "mirsella";
        };
        laptop = {
          HostName = "192.168.1.61";
          User = "mirsella";
        };
        main = {
          HostName = "mirsella.mooo.com";
          Port = 2222;
          User = "mirsella";
        };
        predator = {
          HostName = "mirsella.mooo.com";
          Port = 22;
          User = "mirsella";
        };
      };
    };
    atuin = {
      enable = true;
      package = atuinPkg;
      # Atuin writes a default config on first run; Home Manager must replace it.
      forceOverwriteSettings = true;
      daemon.enable = true;
      settings = {
        auto_sync = true;
        # The daemon owns syncing; never sync from the client.
        sync_frequency = "0";
        key_path = "${config.home.homeDirectory}/.config/${userSecrets.atuin_key}";
        search_mode = "fuzzy";
        filter_mode_shell_up_key_binding = "directory";
        enter_accept = true;
        keymap_mode = "vim-insert";
        prefers_reduced_motion = true;
        stats = {
          common_subcommands = [
            "apt"
            "cargo"
            "composer"
            "dnf"
            "docker"
            "git"
            "go"
            "ip"
            "jj"
            "kubectl"
            "nix"
            "nmcli"
            "npm"
            "pecl"
            "pnpm"
            "podman"
            "port"
            "systemctl"
            "tmux"
            "yarn"
          ];
          common_prefix = [ "sudo" "s" ];
        };
        sync.records = true;
        # Integer seconds; the daemon in nixpkgs accepts only u64.
        daemon.sync_frequency = 5;
        dotfiles.enabled = true;
      };
    };
  };
}
