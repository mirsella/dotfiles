{
  lib,
  fetchurl,
  writeText,
  writeShellScript,
  runCommand,
  host-tools,
  bash,
  coreutils,
  curl,
  findutils,
  gawk,
  gnugrep,
  gnused,
  gnutar,
  gzip,
  procps,
  util-linux,
  appimage-run,
  nix,
  isNixOS ? false,
}:
let
  t3Version = "0.0.46-nightly.20261013.3053";
  t3Revision = "6524a8d43795dc543f7cfb572388cdea3662ad48";
  codexInstaller = fetchurl {
    url = "https://raw.githubusercontent.com/openai/codex/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/scripts/install/install.sh";
    hash = "sha256-FQ489nVoLv6qwRWqN0et0/J4h4ltBM5tC1ZHjYtCi/Y=";
  };
  t3Installer = fetchurl {
    url = "https://raw.githubusercontent.com/pingdotgg/t3code/${t3Revision}/scripts/install.sh";
    hash = "sha256-3Hs93QKuJwDuzj08WMCUWScxGVCRPeFIyN2FcPHRxtU=";
  };
  # Only a seed: activation never replaces an existing, self-updated AppImage.
  desktopImage = fetchurl {
    url = "https://github.com/pingdotgg/t3code/releases/download/v${t3Version}/T3-Code-${t3Version}-x86_64.AppImage";
    hash = "sha512-Mg8631AKjhK0pK0Ph4ecwn9qyzD8WtrMt+022EPiKGsXCwt6xYbAckiiE+PIMvpqt1eVbG3UPvn1qtX287I9XQ==";
  };
  desktopIcon = fetchurl {
    url = "https://raw.githubusercontent.com/pingdotgg/t3code/${t3Revision}/assets/nightly/nightly-universal-1024.png";
    hash = "sha256-flm2OUAW74PtHpRoR3aeAb821AYsXFryV3/T4igoX9k=";
  };
  desktopLauncher = writeShellScript "t3code-nightly" ''
    export TMPDIR="$HOME/dev/.cache/agent-tools"
    exec ${lib.optionalString isNixOS "${lib.getExe appimage-run} "}"$HOME/.local/share/t3code/app/T3-Code-nightly.AppImage" "$@"
  '';
  desktopEntry = writeText "t3code.desktop" ''
    [Desktop Entry]
    Type=Application
    Name=T3 Code Nightly
    Comment=Desktop control surface for coding agents
    Exec=${desktopLauncher} %U
    Icon=${desktopIcon}
    Terminal=false
    Categories=Development;
    StartupWMClass=t3code
    MimeType=x-scheme-handler/t3code;
  '';
  serviceEnvironment = writeText "zz-native-agent-tools.conf" ''
    [Service]
    Environment=T3CODE_CHANNEL=nightly
    EnvironmentFile=-%t/t3code-service-environment.env
    # Run after any shell-environment loader and retain its other exports.
    ExecStartPre=%h/.local/state/agent-tools/installed/bin/host-tools agent-tools-environment %t/t3code-service-environment.env
  '';
  configFile = writeText "agent-tools.json" (
    builtins.toJSON {
      inherit
        codexInstaller
        t3Installer
        t3Version
        desktopImage
        ;
      nixStore = if isNixOS then "${nix}/bin/nix-store" else "/usr/bin/nix-store";
      shell = lib.getExe bash;
      installerPath = lib.makeBinPath [
        bash
        coreutils
        curl
        findutils
        gawk
        gnugrep
        gnused
        gnutar
        gzip
        procps
        util-linux
      ];
    }
  );
in
runCommand "agent-tools"
  {
    meta = {
      description = "Bootstrap writable Codex and nightly T3 CLI/desktop installs";
      mainProgram = "bootstrap-agent-tools";
      platforms = [ "x86_64-linux" ];
    };
  }
  ''
    mkdir -p $out/bin $out/share/agent-tools
    cat > $out/bin/bootstrap-agent-tools <<EOF
    #!${lib.getExe bash}
    exec ${lib.getExe host-tools} bootstrap-agent-tools "$out" "\$@"
    EOF
    chmod +x $out/bin/bootstrap-agent-tools
    cat > $out/bin/t3-settings <<'EOF'
    #!${lib.getExe bash}
    exec ${lib.getExe host-tools} t3-settings "$@"
    EOF
    chmod +x $out/bin/t3-settings
    ln -s ${desktopLauncher} $out/bin/t3code
    ln -s ${desktopLauncher} $out/bin/t3code-nightly
    ln -s ${lib.getExe host-tools} $out/bin/host-tools
    ln -s ${configFile} $out/share/agent-tools/config.json
    ln -s ${desktopEntry} $out/share/agent-tools/t3code.desktop
    ln -s ${serviceEnvironment} $out/share/agent-tools/zz-native-agent-tools.conf
  ''
