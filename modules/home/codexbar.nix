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

  # Plasma can search /usr/share before the Nix profile, selecting an unpatched
  # distro copy. Its user data directory always takes precedence.
  xdg.dataFile."plasma/plasmoids/com.github.psimaker.codexbar".source =
    "${pkgs.codexbar-plasmoid}/share/plasma/plasmoids/com.github.psimaker.codexbar";

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

  # CodexBar atomically replaces its writable config. Keep the SOPS snapshot
  # separate so CLI settings and refreshed cookies cannot replace a secret link.
  sops.secrets.codexbar_config = {
    sopsFile = ../../secrets/codexbar.yaml;
    key = "config_json";
    path = "${config.xdg.configHome}/codexbar/shared-config.json";
    mode = "0600";
  };
}
