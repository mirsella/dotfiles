{ config, lib, pkgs, ... }:
{
  home.packages = [ pkgs.claude-credit-router pkgs.libnotify
    (pkgs.writeShellScriptBin "openai-credit-router" ''
      exec ${lib.getExe pkgs.claude-credit-router} --openai "$@"
    '')
  ];
  systemd.user.services.openai-credit-router = {
    Unit = {
      Description = "OpenAI API project allowance before native Codex subscription usage";
      ConditionPathExists = "${config.xdg.configHome}/openai-credit-router/config.json";
    };
    Service = {
      ExecStart = "${lib.getExe pkgs.claude-credit-router} --openai serve";
      EnvironmentFile = "${config.xdg.configHome}/${(import ../user-secrets.nix).env_secrets}";
      Restart = "on-failure";
      RestartSec = 30;
      UMask = "0077";
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
      ProtectHome = "read-only";
      StateDirectory = "openai-credit-router";
      ReadWritePaths = [ "${config.xdg.stateHome}/openai-credit-router" ];
      RestrictAddressFamilies = [ "AF_UNIX" "AF_INET" "AF_INET6" ];
    };
    Install.WantedBy = [ "default.target" ];
  };
  systemd.user.services.claude-credit-router = {
    Unit = {
      Description = "Claude promotional API credits before native subscription usage";
      ConditionPathExists = "${config.xdg.configHome}/claude-credit-router/config.json";
    };
    Service = {
      ExecStart = "${lib.getExe pkgs.claude-credit-router} serve";
      EnvironmentFile = "${config.xdg.configHome}/${(import ../user-secrets.nix).env_secrets}";
      Restart = "on-failure";
      RestartSec = 30;
      UMask = "0077";
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
      ProtectHome = "read-only";
      StateDirectory = "claude-credit-router";
      ReadWritePaths = [ "${config.xdg.stateHome}/claude-credit-router" ];
      RestrictAddressFamilies = [ "AF_UNIX" "AF_INET" "AF_INET6" ];
    };
    Install.WantedBy = [ "default.target" ];
  };
}
