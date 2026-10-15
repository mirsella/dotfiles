{ lib, pkgs, ... }:
{
  home.packages = [
    pkgs.browser-mcp
    pkgs.computer-use-mcp
  ];
  # Stable marketplace/plugin paths; no development checkout or package-manager
  # cache is required by either harness.
  home.file = {
    ".local/share/agent-mcp/browser-mcp".source = "${pkgs.browser-mcp}/share/browser-mcp";
    ".local/share/agent-mcp/computer-use-mcp".source =
      "${pkgs.computer-use-mcp}/share/computer-use-mcp";
  };
  xdg.configFile."environment.d/50-opencode.conf" = {
    force = true;
    text = ''
      OPENCODE_BROWSER_EXECUTABLE=${lib.getExe pkgs.chromium}
    '';
  };
}
