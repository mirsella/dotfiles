{
  lib,
  stdenvNoCC,
  fetchFromGitHub,
}:
stdenvNoCC.mkDerivation {
  pname = "leanrea";
  version = "unstable-2026-10-17";

  src = fetchFromGitHub {
    owner = "aicodeking";
    repo = "leanrea";
    rev = "09ba4aa0217f82cd880f855a60b877f59f39b22b";
    hash = "sha256-YY4nEElHOgZQ6ELCw/5qUzcgKf7mFGW6CS+tP8GyMiM=";
  };

  postPatch = ''
    # Prefer the Nix-provided CLIs over runtime package downloads.
    substituteInPlace leanrea/SKILL.md \
      --replace-fail 'npx -y @electron/asar' 'asar' \
      --replace-fail 'npx -y @electron/fuses' 'electron-fuses' \
      --replace-fail 'npx -y prettier' 'prettier' \
      --replace-fail '(install: `dotnet tool install -g ilspycmd`)' '(installed by Nix)' \
      --replace-fail 'checksec: `pwn checksec`' 'checksec: `checksec`' \
      --replace-fail 'or `brew install <x>` if the user allows it' 'or ask for the dotfiles flake to add it if the user allows it' \
      --replace-fail '`brew install jadx apktool radare2 binwalk wabt wireshark ghidra` · `npm i -g @electron/asar` · `dotnet tool install -g ilspycmd` · `cargo install cargo-audit`.' 'The workstation Home Manager profiles install jadx, apktool, radare2, binwalk, wabt, tshark, Ghidra, asar, electron-fuses, ilspycmd and cargo-audit through Nix. Prefer those commands and ask for the dotfiles flake to add a missing tool instead of running brew, npm, dotnet tool or cargo install.' \
      --replace-fail 'Ghidra `analyzeHeadless $D p -import T -postScript DecompileAll.java -deleteProject` or ' ""
  '';

  installPhase = ''
    runHook preInstall
    install -Dm644 -t "$out/share/leanrea" leanrea/SKILL.md LICENSE
    runHook postInstall
  '';

  meta = {
    description = "Reverse engineering skill that uses plain local CLI tools instead of an MCP server";
    homepage = "https://github.com/aicodeking/leanrea";
    license = lib.licenses.mit;
    platforms = lib.platforms.all;
  };
}
