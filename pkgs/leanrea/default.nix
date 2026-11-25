{
  lib,
  stdenvNoCC,
  fetchFromGitHub,
  coreutils,
  ghidra,
  gnugrep,
  writeShellApplication,
}:
let
  decompile = writeShellApplication {
    name = "leanrea-decompile";
    runtimeInputs = [ coreutils gnugrep ];
    text = ''
      if [[ $# != 2 || ! -f $1 || -z $2 ]]; then
        echo 'Usage: leanrea-decompile BINARY FUNCTION_NAME|0xGHIDRA_ENTRY_ADDRESS' >&2
        exit 2
      fi
      binary=$(realpath -- "$1")
      # Ghidra refuses project paths with dot-prefixed components (e.g. .scratch).
      project=$(mktemp -d /tmp/leanrea-ghidra.XXXXXXXX)
      trap 'rm -rf -- "$project"' EXIT
      output="$project/function.c"

      # Ghidra can exit successfully even when a script fails. Require its output.
      if ! ${ghidra}/bin/ghidra-analyzeHeadless "$project" leanrea \
        -import "$binary" -readOnly -analysisTimeoutPerFile 60 \
        -scriptPath ${./scripts} -postScript LeanreaDecompile.java "$2" "$output" \
        -log "$project/ghidra.log" -scriptlog "$project/script.log" >"$project/run.log" 2>&1 \
        || [[ ! -s "$output" ]]; then
        echo 'Ghidra did not produce pseudocode:' >&2
        if ! grep -m 8 -E '^(ERROR|Caused by:)' "$project/run.log" >&2; then
          tail -n 20 "$project/run.log" >&2
        fi
        exit 1
      fi
      cat "$output"
    '';
  };
in
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
      --replace-fail 'checksec: `pwn checksec`' 'checksec: `checksec file T`' \
      --replace-fail 'or `brew install <x>` if the user allows it' 'or ask for the dotfiles flake to add it if the user allows it' \
      --replace-fail '`brew install jadx apktool radare2 binwalk wabt wireshark ghidra` · `npm i -g @electron/asar` · `dotnet tool install -g ilspycmd` · `cargo install cargo-audit`.' 'The workstation Home Manager profiles install jadx, apktool, radare2, binwalk, wabt, tshark, Ghidra, asar, electron-fuses, ilspycmd and cargo-audit through Nix. Prefer those commands and ask for the dotfiles flake to add a missing tool instead of running brew, npm, dotnet tool or cargo install.' \
      --replace-fail 'Ghidra `analyzeHeadless $D p -import T -postScript DecompileAll.java -deleteProject` or `r2 -qc' 'prefer headless Ghidra (`leanrea-decompile T FUNCTION_NAME` or `leanrea-decompile T 0xGHIDRA_ENTRY_ADDRESS`; use Ghidra’s loaded address, not a file offset). It writes one function as C-like pseudocode to stdout without running the target. For a quick fallback use `r2 -qc' \
      --replace-fail '## First move: one call each, paste and run' $'## Workstation tools on PATH\nOn Arch, if the agent shell does not see Home Manager tools, add `export PATH="$PATH:$HOME/.nix-profile/bin"` to each independent shell call. On NixOS, use `/etc/profiles/per-user/$USER/bin` if it is missing.\n\n## First move: one call each, paste and run' \
      --replace-fail '`hermes-dec` / `hbc-decompiler`' '`hbc-decompiler T out.js`; for disassembly, use `hbc-disassembler T out.txt`. Read the output in slices' \
      --replace-fail 'Dart AOT: `blutter`' 'Dart AOT: `blutter` if separately provisioned for the target Dart version'
  '';

  installPhase = ''
    runHook preInstall
    install -Dm644 -t "$out/share/leanrea" leanrea/SKILL.md LICENSE
    mkdir -p "$out/bin"
    ln -s ${lib.getExe decompile} "$out/bin/leanrea-decompile"
    runHook postInstall
  '';

  meta = {
    description = "Reverse engineering skill that uses plain local CLI tools instead of an MCP server";
    homepage = "https://github.com/aicodeking/leanrea";
    license = lib.licenses.mit;
    platforms = lib.platforms.all;
  };
}
