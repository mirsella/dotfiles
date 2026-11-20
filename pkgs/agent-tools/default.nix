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
  nodejs,
  isNixOS ? false,
}:
let
  # Anthropic publishes no versioned installer URL; this hash pins the current
  # launcher script, and the bootstrap passes claudeVersion to select the seed.
  claudeInstaller = fetchurl {
    url = "https://claude.ai/install.sh";
    hash = "sha256-OmjTQGz2dOF77RczpNzzeAXi5H2HQXcAAH1+GqdmqUQ=";
  };
  claudeVersion = "2.1.316";
  desktopIcon = fetchurl {
    url = "https://raw.githubusercontent.com/pingdotgg/t3code/d2c440312e5f39d59827ed466838a623516454d6/assets/nightly/nightly-universal-1024.png";
    hash = "sha256-flm2OUAW74PtHpRoR3aeAb821AYsXFryV3/T4igoX9k=";
  };
  desktopLauncher = writeShellScript "t3code-nightly" ''
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
  # The user manager can inherit a PATH without any NixOS profiles. T3
  # and its provider updaters invoke tools like cp, curl and systemctl by name.
  nixosServicePath = "/run/wrappers/bin:/etc/profiles/per-user/%u/bin:/run/current-system/sw/bin";
  serviceEnvironment = writeText "zz-native-agent-tools.conf" ''
    [Unit]
    Wants=sops-nix.service
    After=sops-nix.service

    [Service]
    EnvironmentFile=-%t/t3code-service-environment.env
    # The user manager imports environment.d once at startup, before sops-nix
    # installs the runtime secrets.
    EnvironmentFile=-%h/.config/environment.d/55-secrets.conf
    # Run after any shell-environment loader and retain its other exports.
    ExecStartPre=%h/.local/state/agent-tools/installed/bin/host-tools agent-tools-environment %t/t3code-service-environment.env${lib.optionalString isNixOS " --path-prefix ${nixosServicePath}"}
  '';
  configFile = writeText "agent-tools.json" (
    builtins.toJSON {
      inherit claudeInstaller claudeVersion;
      curl = lib.getExe curl;
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
      description = "Bootstrap writable OpenCode, Codex, Claude Code and T3 installs";
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
    # Default pruning imports T3's own settings schema bundle.
    export T3_SETTINGS_NODE=${lib.getExe nodejs}
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
