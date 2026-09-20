{ pkgs, ... }:
{
  home.sessionPath = [ "$HOME/.local/share/cargo/bin" ];

  home.file.".rustup".source = "/home/mirsella/.local/share/rustup";
  home.file.".cargo".source = "/home/mirsella/.local/share/cargo";

  home.packages = with pkgs; [
    age
    aspell
    aspellDicts.en
    ast-grep
    atuin
    bat
    carapace
    cargo-expand
    cargo-update
    cargo-watch
    chafa
    chezmoi
    delta
    diffstat
    difftastic
    dioxus-cli
    dust
    fd
    fzf
    gcc
    gh
    graphviz
    gtrash
    hunspell
    hunspellDicts.en_US
    inxi
    jq
    jujutsu
    kache
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
    nodemon
    nushell
    ouch
    pnpm
    prettier
    python3
    python3Packages.pynvim
    rclone
    rift-cli
    ripgrep
    rtk
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
    vimv
    wrangler
    yt-dlp
    zip
    zoxide
  ];
}
