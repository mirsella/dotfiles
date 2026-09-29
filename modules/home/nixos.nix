{ pkgs, config, lib, ... }:
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
    atuin
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
    dioxus-cli
    dust
    fd
    fish
    fzf
    gcc
    gh
    go
    graphviz
    gtrash
    hunspell
    hunspellDicts.en_US
    inxi
    jdk21
    jq
    jujutsu
    julia
    just
    kotlin
    lazygit
    lazyjj
    lsd
    lspmux
    markdownlint-cli
    mergiraf
    mermaid-cli
    mold
    neovim
    nodejs
    nushell
    ouch
    php
    pnpm
    prettier
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
