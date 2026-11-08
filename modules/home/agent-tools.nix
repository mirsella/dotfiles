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
    home.packages = [ config.programs.agent-tools.package ];
    home.sessionPath = lib.mkBefore [ "${config.home.homeDirectory}/.local/bin" ];
    home.activation.agentTools = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
      run ${lib.getExe config.programs.agent-tools.package}
    '';
    home.activation.agentToolsTimers = lib.hm.dag.entryAfter [ "reloadSystemd" ] ''
      if ${pkgs.systemd}/bin/systemctl --user show-environment >/dev/null 2>&1; then
        run ${pkgs.systemd}/bin/systemctl --user start --no-block \
          agent-tools-backups.timer scratch-prune.timer
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
    systemd.user.services.scratch-prune = {
      Unit.Description = "Remove stale task scratch directories";
      Service = {
        Type = "oneshot";
        ExecStart = "${lib.getExe pkgs.host-tools} scratch-prune";
      };
    };
    systemd.user.timers.scratch-prune = {
      Unit.Description = "Prune task scratch directories daily";
      Timer = {
        OnCalendar = "12:00";
        Persistent = true;
      };
      Install.WantedBy = [ "timers.target" ];
    };
  };
}
