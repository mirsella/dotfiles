{
  isNixOS,
  lib,
  pkgs,
  ...
}:
{
  # Browser MCP and Computer Use MCP install themselves from their published
  # packages; only the browser executable needs pinning for session startup.
  xdg.configFile."environment.d/50-opencode.conf" = {
    force = true;
    text = ''
      OPENCODE_BROWSER_EXECUTABLE=${if isNixOS then lib.getExe pkgs.helium else "/usr/bin/helium-browser"}
    '';
  };
}
