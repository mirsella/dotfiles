{ config, lib, pkgs, ... }:
let
  executable = lib.getExe pkgs.hex;
  settings = "${pkgs.host-tools}/bin/host-tools hex-settings";
  supportDir = "${config.xdg.dataHome}/hex";
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
  home.packages = [ pkgs.hex ];
  home.file.".local/bin/hex".source = executable;
  home.file.".local/bin/hex-settings".source = pkgs.writeShellScript "hex-settings" ''
    exec ${settings} "$@"
  '';
  # Both applications verify the same Cohere Q8_0 artifact. Keep one copy on disk.
  home.file.".t3/userdata/speech/models/cohere-transcribe-03-2026-Q8_0.gguf".source =
    config.lib.file.mkOutOfStoreSymlink "${supportDir}/models/cohere-transcribe-03-2026-Q8_0.gguf";

  xdg.dataFile."applications/anomaly.hex.desktop".source = "${desktop}/share/applications/anomaly.hex.desktop";

  # One login owner. Hex deliberately leaves foreign/symlinked entries alone.
  xdg.configFile."autostart/anomaly.hex.desktop".text = ''
    [Desktop Entry]
    Type=Application
    Name=Hex (started by Home Manager's hex.service)
    Hidden=true
  '';

  systemd.user.services.hex = {
    Unit = {
      Description = "Hex voice dictation";
      After = [ "graphical-session.target" ];
      PartOf = [ "graphical-session.target" ];
      StartLimitIntervalSec = 60;
      StartLimitBurst = 5;
    };
    Service = {
      Type = "notify";
      NotifyAccess = "main";
      ExecStartPre = "${settings} sync --no-capture --if-present";
      ExecStart = "${executable} service";
      Environment = [ "HEX_APPLICATION_SUPPORT_DIR=${supportDir}" ];
      EnvironmentFile = "-${supportDir}/session.env";
      Restart = "on-failure";
      RestartSec = 3;
      TimeoutStartSec = 35;
      TimeoutStopSec = 30;
      UMask = "0077";
    };
    Install.WantedBy = [ "graphical-session.target" ];
  };

  # The selected model is device-local data, not a file to put in Git.
  systemd.user.services.hex-model = {
    Unit = {
      Description = "Ensure Hex's selected speech model is installed";
      After = [ "hex.service" ];
      Wants = [ "hex.service" ];
      PartOf = [ "graphical-session.target" "hex.service" ];
      StartLimitIntervalSec = 0;
    };
    Service = {
      Type = "oneshot";
      ExecStart = "${pkgs.host-tools}/bin/host-tools hex-model --hex ${executable}";
      Environment = [ "HEX_APPLICATION_SUPPORT_DIR=${supportDir}" ];
      Restart = "on-failure";
      RestartSec = 60;
      RemainAfterExit = true;
      TimeoutStartSec = 600;
      UMask = "0077";
    };
    Install.WantedBy = [ "graphical-session.target" ];
  };
}
