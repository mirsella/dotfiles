{
  config,
  isNixOS,
  lib,
  pkgs,
  ...
}:
let
  skillsDir = ../../dotfiles/private_dot_config/opencode/skills;
  skillDirs = lib.filterAttrs (_: type: type == "directory") (builtins.readDir skillsDir);
in
{
  imports = [ ./agent-mcp.nix ];

  options.programs.agent-tools.package = lib.mkOption {
    type = lib.types.package;
    default = pkgs.agent-tools.override { inherit isNixOS; };
    description = "Bootstrap and launchers for writable OpenCode, Codex, Claude Code and T3 installs.";
  };

  config = {
    # Claude Code reads user memory and personal skills from ~/.claude, never
    # from ~/.agents. Link the same chezmoi-owned sources into place so every
    # agent reads one copy.
    home.file =
      {
        ".agents/skills" = {
          source = config.lib.file.mkOutOfStoreSymlink "${config.xdg.configHome}/opencode/skills";
          force = true;
        };
        ".claude/CLAUDE.md".source =
          config.lib.file.mkOutOfStoreSymlink "${config.xdg.configHome}/opencode/AGENTS.md";
        ".local/bin/t3-settings" = {
          source = "${config.programs.agent-tools.package}/bin/t3-settings";
          force = true;
        };
      }
      // lib.mapAttrs' (name: _: lib.nameValuePair ".claude/skills/${name}" {
        source = config.lib.file.mkOutOfStoreSymlink "${config.xdg.configHome}/opencode/skills/${name}";
      }) skillDirs;
    # These launchers/bootstrap are immutable; native application releases stay
    # writable and existing self-updated installs are never replaced.
    home.packages = [
      config.programs.agent-tools.package
      # `sweep`: when agent workspaces get cleaned up, manual overrides, and the
      # hourly cleanup itself (`sweep auto`).
      (pkgs.writeShellScriptBin "sweep" ''
        exec ${lib.getExe pkgs.host-tools} sweep "$@"
      '')
    ];
    home.sessionPath = lib.mkBefore [ "${config.home.homeDirectory}/.local/bin" ];
    home.activation.agentTools = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
      run ${lib.getExe config.programs.agent-tools.package}
    '';
    home.activation.agentToolsTimers = lib.hm.dag.entryAfter [ "reloadSystemd" ] ''
      if ${pkgs.systemd}/bin/systemctl --user show-environment >/dev/null 2>&1; then
        run ${pkgs.systemd}/bin/systemctl --user start --no-block \
          agent-tools-backups.timer sweep.timer
      fi
    '';
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
    # The only owner of agent workspace cleanup: T3's portable settings keep its
    # worktree cleanup off and leave it only logs and browser artifacts. It keeps
    # anything it cannot verify, and notifies once when an idle workspace holds
    # unsaved work or a run fails; the state directory remembers what it reported.
    systemd.user.services.sweep = {
      Unit.Description = "Remove idle agent scratch folders and worktrees";
      Service = {
        Type = "oneshot";
        ExecStart = "${lib.getExe pkgs.host-tools} sweep auto";
        StateDirectory = "sweep";
        Nice = 19;
        IOSchedulingClass = "idle";
        Environment = [
          # notify-send comes from the host; Nix supplies it where the host has none.
          "PATH=/run/current-system/sw/bin:/usr/bin:/bin:${lib.makeBinPath [ pkgs.libnotify pkgs.coreutils ]}"
          "DBUS_SESSION_BUS_ADDRESS=unix:path=%t/bus"
        ];
      };
    };
    systemd.user.timers.sweep = {
      Unit.Description = "Prune idle agent workspaces hourly";
      Timer = {
        OnCalendar = "hourly";
        RandomizedDelaySec = "5m";
        Persistent = true;
      };
      Install.WantedBy = [ "timers.target" ];
    };
  };
}
