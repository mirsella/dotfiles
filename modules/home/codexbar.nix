{
  config,
  isNixOS,
  lib,
  pkgs,
  ...
}:
let
  executable = if isNixOS then lib.getExe pkgs.codexbar-cli else "/usr/bin/codexbar";
  launcher = pkgs.writeShellScript "codexbar" ''
    set -eu
    . ${lib.escapeShellArg config.sops.secrets.codexbar_opencode_env.path}
    : "''${OPENCODE_API_KEY:?CodexBar OpenCode Go key is missing}"
    export OPENCODE_API_KEY
    exec ${lib.escapeShellArg executable} "$@"
  '';
in
{
  # Pacman owns both application packages on Arch.
  home.packages = lib.optionals isNixOS [
    pkgs.codexbar-cli
    pkgs.codexbar-plasmoid
  ];

  home.file.".local/bin/codexbar" = {
    source = launcher;
    force = true;
  };

  sops.secrets.codexbar_opencode_env = {
    sopsFile = ../../secrets/codexbar.yaml;
    key = "opencode_env";
    path = "${config.xdg.configHome}/codexbar/opencode.env";
    mode = "0600";
  };
}
