{
  config,
  isNixOS,
  lib,
  pkgs,
  ...
}:
{
  options.programs.agent-tools.package = lib.mkOption {
    type = lib.types.package;
    default = pkgs.agent-tools.override { inherit isNixOS; };
    description = "Bootstrap and launchers for writable Codex and nightly T3 installs.";
  };

  config = {
    # These launchers/bootstrap are immutable; application releases stay writable.
    home.packages = [ config.programs.agent-tools.package ];
    home.sessionPath = lib.mkBefore [ "${config.home.homeDirectory}/.local/bin" ];
    home.activation.agentTools = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
      run ${lib.getExe config.programs.agent-tools.package}
    '';
  };
}
