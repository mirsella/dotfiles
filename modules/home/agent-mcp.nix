{ config, lib, pkgs, ... }:
let
  inventory = builtins.fromJSON (builtins.readFile ../../dotfiles/private_dot_config/agent-tools/mcp.json);
  # Follow the installed release when current moves, without fetching packages
  # in the short-lived lifecycle hooks.
  browserRoot = "${config.home.homeDirectory}/.local/share/browser-mcp/current";
  # Adapt Claude's hook envelope to the published browser hook protocol. Keep
  # Claude IDs distinct from Codex IDs and use the broker's queued cleanup.
  browserHook = "${lib.getExe pkgs.host-tools} claude-browser-hook "
    + lib.escapeShellArg "${browserRoot}/vendor/browser-mcp";
  claudeServers = lib.mapAttrs (_: server:
    builtins.removeAttrs server [ "enabled" "toolTimeoutSec" "oauth" ]
  ) (lib.filterAttrs (_: server: server.enabled) inventory.servers);
  migratedServerNames = pkgs.writeText "claude-managed-mcp-names.json"
    (builtins.toJSON ([ "browser" ] ++ builtins.attrNames inventory.servers));
in
{
  # Home Manager's personal plugin supplies generated MCP configuration without
  # tracking ~/.claude.json, which also contains login and runtime state.
  programs.claude-code = {
    enable = true;
    package = null; # Preserve the writable native install and its self-updater.
    mutableSettings = true;
    mcpServers = claudeServers // {
      browser = { type = "stdio"; command = "${browserRoot}/vendor/browser-mcp"; };
    };
    settings.env = { MCP_TIMEOUT = "40000"; MCP_TOOL_TIMEOUT = "540000"; };
    settings.hooks = {
      PreToolUse = [{
        matcher = "^mcp__(browser|plugin_hm_browser)__.*$";
        hooks = [{ type = "command"; command = "${browserHook}"; timeout = 5; }];
      }];
      SessionEnd = [{
        hooks = [{ type = "command"; command = "${browserHook}"; timeout = 3; }];
      }];
    };
  };
  home.file.".claude/skills/browser-automation".source =
    config.lib.file.mkOutOfStoreSymlink "${browserRoot}/skills/browser-automation";
  # Old user-scoped entries override plugin configuration. Retire only these
  # managed names; retain authentication, project state, and unrelated MCPs.
  home.activation.claudeManagedMcp = lib.hm.dag.entryAfter [ "linkGeneration" ] (
    lib.hm.generators.mkImpureConfigMerger {
      inherit pkgs;
      path = "${config.home.homeDirectory}/.claude.json";
      staticSettings = migratedServerNames;
      format = "json";
      empty = "{}";
      mode = "600";
      jqOperation = ''$dynamic | delpaths($static | map(["mcpServers", .]))'';
    }
  );
}
