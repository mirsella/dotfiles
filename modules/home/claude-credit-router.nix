{ config, lib, pkgs, ... }:
let
  vendors = {
    claude = "Claude promotional API credits before native subscription usage";
    openai = "OpenAI API project allowance before native Codex subscription usage";
  };
in {
  xdg.configFile = lib.mapAttrs'
    (vendor: _: lib.nameValuePair "opencode/credit-routing/${vendor}.path" {
      text = "${pkgs.opencode-extensions}/share/opencode/credit-routing-${vendor}";
    }) vendors;
  home.packages = [ pkgs.claude-credit-router pkgs.libnotify
    (pkgs.writeShellScriptBin "openai-credit-router" ''
      exec ${lib.getExe pkgs.claude-credit-router} --openai "$@"
    '')
  ];
  systemd.user.services = lib.mapAttrs' (vendor: description:
    lib.nameValuePair "${vendor}-credit-router" {
    Unit = {
      Description = description;
      ConditionPathExists = "${config.xdg.configHome}/${vendor}-credit-router/config.json";
    };
    Service = {
      ExecStart = "${lib.getExe pkgs.claude-credit-router} ${lib.optionalString (vendor == "openai") "--openai "}serve";
      EnvironmentFile = "${config.xdg.configHome}/${(import ../user-secrets.nix).env_secrets}";
      Restart = "on-failure";
      RestartSec = 30;
      UMask = "0077";
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
      ProtectHome = "read-only";
      # A distinct parent avoids systemd's legacy ~/.config migration heuristic.
      StateDirectory = "credit-router";
      ReadWritePaths = [ "${config.xdg.stateHome}/credit-router" ];
      RestrictAddressFamilies = [ "AF_UNIX" "AF_INET" "AF_INET6" ];
    };
    Install.WantedBy = [ "default.target" ];
  }) vendors;
}
