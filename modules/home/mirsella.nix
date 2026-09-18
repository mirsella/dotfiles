{ pkgs, ... }:
{
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
    };
  };

  xdg.configFile =
    let
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
    builtins.listToAttrs (
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
