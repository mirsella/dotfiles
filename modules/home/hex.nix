{ config, lib, pkgs, ... }:
let
  executable = lib.getExe pkgs.hex;
  supportDir = "${config.xdg.dataHome}/hex";
  model = "cohere-transcribe-03-2026-Q8_0.gguf";
  desktop = pkgs.makeDesktopItem {
    name = "anomaly.hex";
    desktopName = "Hex";
    comment = "Local voice dictation";
    exec = executable;
    icon = "${pkgs.hex}/share/icons/hicolor/512x512/apps/anomaly.hex.png";
    categories = [ "Utility" "Accessibility" ];
    startupNotify = true;
    startupWMClass = "anomaly.hex";
    actions.quit = {
      name = "Quit Hex";
      exec = "${executable} quit";
    };
  };
in
{
  # Upstream's Home Manager module (flake input `hex`) owns hex.service.
  programs.hex = {
    enable = true;
    autostart = true;
    package = pkgs.hex;
  };

  home.file.".local/bin/hex".source = executable;
  home.file.".local/bin/hex-settings".source = pkgs.writeShellScript "hex-settings" ''
    exec ${pkgs.host-tools}/bin/host-tools hex-settings "$@"
  '';
  # Both applications verify the same Cohere Q8_0 artifact. Keep one copy on disk.
  home.file.".t3/userdata/speech/models/${model}".source =
    config.lib.file.mkOutOfStoreSymlink "${supportDir}/models/${model}";

  xdg.dataFile."applications/anomaly.hex.desktop".source = "${desktop}/share/applications/anomaly.hex.desktop";

  # hex.service starts Hex at login. Hex's own Launch at login entry would run
  # the unwrapped store binary, so keep it disabled.
  xdg.configFile."autostart/anomaly.hex.desktop".text = ''
    [Desktop Entry]
    Type=Application
    Name=Hex (started by Home Manager's hex.service)
    Hidden=true
  '';

  # One 2.4 GB download per machine. Hex writes the marker once the file is verified.
  home.activation.hexModel = lib.hm.dag.entryAfter [ "reloadSystemd" ] ''
    if [ ! -e ${lib.escapeShellArg "${supportDir}/models/${model}.verified.json"} ]; then
      if ! run ${pkgs.coreutils}/bin/env HEX_APPLICATION_SUPPORT_DIR=${lib.escapeShellArg supportDir} \
        ${executable} model install cohere_transcribe --language en; then
        warnEcho "Hex didn't install its model; run: hex model install cohere_transcribe --language en"
      fi
    fi
  '';
}
