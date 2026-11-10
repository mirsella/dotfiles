{ config, lib, pkgs, ... }:
let
  inventory = builtins.fromJSON (builtins.readFile ../../dotfiles/private_dot_config/agent-tools/mcp.json);
  # One pinned browser-mcp release for Claude, Codex and OpenCode. Codex trusts
  # its hooks by content hash, so a release bump must update both hashes.
  browser = inventory.browser;
  browserRoot = pkgs.runCommand "browser-mcp-${browser.version}" {
    src = pkgs.fetchurl {
      url = "https://registry.npmjs.org/${browser.package}/-/browser-mcp-${browser.version}.tgz";
      hash = browser.npmIntegrity;
    };
    nativeBuildInputs = [ pkgs.host-tools ];
  } ''
    mkdir "$out"
    tar -xzf "$src" --strip-components=1 -C "$out"
    actual=$(host-tools codex-plugin-hash "$out")
    if [ "$actual" != ${lib.escapeShellArg browser.codexContentHash} ]; then
      echo "browser-mcp ${browser.version}: Codex content hash is $actual; update codexContentHash in agent-tools/mcp.json" >&2
      exit 1
    fi
  '';
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
  home.file.".claude/skills/browser-automation".source = "${browserRoot}/skills/browser-automation";
  # Codex installs its plugin from this stable path (see dot_codex/private_config.toml.tmpl).
  home.file.".local/share/browser-mcp/current".source = browserRoot;
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
