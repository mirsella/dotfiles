{ lib, config, pkgs, gitSigningKey, isNixOS, ... }:
let
  lspmuxBin =
    if isNixOS then "${pkgs.lspmux}/bin/lspmux"
    else "${config.home.homeDirectory}/.local/share/cargo/bin/lspmux";
  mkdirBin = if isNixOS then "${pkgs.coreutils}/bin/mkdir" else "/usr/bin/mkdir";
  rcloneBin = if isNixOS then "${pkgs.rclone}/bin/rclone" else "/usr/bin/rclone";
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

  systemd.user.services = {
    lspmux = {
      Unit.Description = "Language server multiplexer server";
      Service = {
        Type = "simple";
        ExecStart = "${lspmuxBin} server";
        Restart = "on-failure";
        RestartSec = "5s";
      };
      Install.WantedBy = [ "default.target" ];
    };
  } // lib.mapAttrs' (remote: description: lib.nameValuePair "rclone-${remote}" {
    Unit = {
      Description = "${description} mount";
      After = [ "network.target" ] ++ lib.optional (!isNixOS) "sops-nix.service";
      Wants = lib.optional (!isNixOS) "sops-nix.service";
    };
    Service = {
      # rclone reports readiness and unmounts on SIGTERM itself.
      Type = "notify";
      ExecStartPre = "${mkdirBin} -p %h/Documents/${remote}";
      ExecStart = "${rcloneBin} mount --vfs-cache-mode full ${remote}: %h/Documents/${remote}";
      SuccessExitStatus = "143";
      Restart = "always";
      RestartSec = 3;
    };
  }) {
    gdrive = "Google Drive";
    gdrive-voxride = "Voxride Google Drive";
  };

  programs = {
    git = {
      enable = true;
      package = if isNixOS then pkgs.git else null;
      settings = lib.recursiveUpdate {
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
      } (lib.optionalAttrs (gitSigningKey != null) {
        commit.gpgsign = true;
        user.signingkey = gitSigningKey;
      });
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
        predator = {
          HostName = "192.168.1.19";
          User = "mirsella";
        };
      };
    };
  };
}
