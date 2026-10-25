{ pkgs, ... }:
{
  # leanrea is a prompt for tools the agent runs itself, so install the Linux
  # toolchain rather than an MCP server.
  home.packages = with pkgs; [
    ghidra
    jadx
    apktool
    radare2
    binwalk
    unblob
    wabt
    wireshark-cli
    asar
    electron-fuses
    ilspycmd
    cargo-audit
    checksec
    frida-tools
    android-tools
    icoutils
    libplist
    binutils
    file
    go
    sqlite
    p7zip
    unzip
    ripgrep
    jq
    nodejs
    prettier
    llvmPackages.llvm
  ];

  home.file.".config/opencode/skills/leanrea" = {
    source = "${pkgs.leanrea}/share/leanrea";
    recursive = true;
  };
}
