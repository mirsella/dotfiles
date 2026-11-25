{ pkgs, config, lib, ... }:
let
  # Keep the key in the session directory so SSH and boot services work
  # without an unlocked Secret Service. The provider is forced: any other one
  # would log the shared session out. An explicit session directory is kept.
  # Arch installs the same wrapper from arch/maintenance/pass-cli.
  proton-pass-cli = pkgs.symlinkJoin {
    name = "proton-pass-cli-fs-${pkgs.proton-pass-cli.version}";
    paths = [ pkgs.proton-pass-cli ];
    nativeBuildInputs = [ pkgs.makeWrapper ];
    postBuild = ''
      wrapProgram "$out/bin/pass-cli" \
        --set PROTON_PASS_KEY_PROVIDER fs \
        --set-default PROTON_PASS_SESSION_DIR ${lib.escapeShellArg "${config.xdg.dataHome}/proton-pass-cli/fs"}
    '';
    meta.mainProgram = "pass-cli";
  };
in
{
  home.sessionPath = [ "$HOME/.local/share/cargo/bin" ];

  home.sessionVariables = {
    CARGO_HOME = "$HOME/.local/share/cargo";
    RUSTUP_HOME = "$HOME/.local/share/rustup";
  };

  home.file.".rustup".source = config.lib.file.mkOutOfStoreSymlink "${config.home.homeDirectory}/.local/share/rustup";

  home.packages = with pkgs; [
    age
    aspell
    aspellDicts.en
    ast-grep
    bat
    bevy-cli
    biome
    bun
    carapace
    cargo-binstall
    cargo-expand
    cargo-ndk
    cargo-update
    cargo-watch
    chafa
    chezmoi
    clang_18
    cmake
    delta
    deno
    diffstat
    difftastic
    # Deno also ships a dx alias; keep dx as the Dioxus CLI.
    (lib.hiPrio dioxus-cli)
    dust
    fd
    fish
    fzf
    # Prefer GNU's cc/c++/cpp; Clang remains available as clang/clang++.
    (lib.setPrio 0 gcc)
    gh
    go
    graphviz
    gtrash
    hunspell
    hunspellDicts.en_US
    inxi
    jq
    jujutsu
    just
    lazygit
    lazyjj
    lsd
    lspmux
    markdownlint-cli
    # mergiraf 0.19.1's test suite aborts on the current nixpkgs; install without checks.
    (mergiraf.overrideAttrs { doCheck = false; })
    mold
    neovim
    nodejs
    nushell
    ouch
    php
    pnpm
    prettier
    proton-pass-cli
    protonmail-cli
    python3
    python3Packages.pynvim
    pyright
    rclone
    ripgrep
    rustup
    sea-orm-cli
    secretspec
    sfw
    starship
    stuff
    tealdeer
    tmux
    tree-sitter
    unzip
    uv
    valgrind
    vimv
    wasm-bindgen-cli
    wasm-tools
    (lib.hiPrio wild)
    wrangler
    wtp
    yt-dlp
    zip
    zoxide
    zsh
  ];
}
