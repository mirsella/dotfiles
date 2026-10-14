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
    description = "Bootstrap and launchers for writable OpenCode, Codex and nightly T3 installs.";
  };

  config = {
    home.file.".agents/skills" = {
      source = config.lib.file.mkOutOfStoreSymlink "${config.xdg.configHome}/opencode/skills";
      force = true;
    };
    # These launchers/bootstrap are immutable; application releases stay writable.
    home.packages = [ config.programs.agent-tools.package ];
    home.file.".local/bin/t3-settings" = {
      source = "${config.programs.agent-tools.package}/bin/t3-settings";
      force = true;
    };
    home.sessionPath = lib.mkBefore [ "${config.home.homeDirectory}/.local/bin" ];
    home.activation.agentTools = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
      run ${lib.getExe config.programs.agent-tools.package}
    '';
    home.activation.agentToolsSetup = lib.hm.dag.entryAfter [ "reloadSystemd" ] ''
      if ${pkgs.systemd}/bin/systemctl --user show-environment >/dev/null 2>&1; then
        run ${pkgs.systemd}/bin/systemctl --user start --no-block \
          agent-tools-setup.service agent-tools-setup.path agent-tools-backups.timer
      fi
    '';
    systemd.user.services.agent-tools-setup = {
      Unit = {
        Description = "Restore portable agent preferences and local OpenCode credentials";
        After = [ "opencode.service" "t3code.service" ];
        Wants = [ "opencode.service" "t3code.service" ];
      };
      Service = {
        Type = "oneshot";
        ExecStart = "${lib.getExe pkgs.host-tools} agent-tools-setup";
        Environment = "PATH=${lib.makeBinPath [ pkgs.systemd ]}:${config.home.homeDirectory}/.local/bin";
        TimeoutStartSec = "120s";
      };
      Install.WantedBy = [ "default.target" ];
    };
    systemd.user.paths.agent-tools-setup = {
      Unit.Description = "Restore agent settings when their local inputs change";
      Path.PathChanged = [
        "%h/.config/t3code/portable-settings.json"
        "%h/.config/opencode/service.json"
        "%h/.t3/userdata/server-runtime.json"
      ];
      Install.WantedBy = [ "default.target" ];
    };
    systemd.user.services.agent-tools-backups = {
      Unit.Description = "Prune enrolled migration backups beyond retention";
      Service = {
        Type = "oneshot";
        ExecStart = "${lib.getExe pkgs.host-tools} agent-tools-backups prune";
        UMask = "0077";
      };
    };
    systemd.user.timers.agent-tools-backups = {
      Unit.Description = "Apply migration backup retention weekly";
      Timer = {
        OnCalendar = "weekly";
        Persistent = true;
      };
      Install.WantedBy = [ "timers.target" ];
    };
  };
}
