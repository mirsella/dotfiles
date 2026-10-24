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
  # Pacman owns the CLI on Arch. The widget comes from Nix on every host so
  # the local pace patch applies until upstream ships it.
  home.packages = [ pkgs.codexbar-plasmoid ] ++ lib.optionals isNixOS [ pkgs.codexbar-cli ];

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
