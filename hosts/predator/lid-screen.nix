{ config, pkgs, ... }:
let
  refreshScreen = "${config.systemd.package}/bin/systemctl --no-block restart predator-lid-screen.service";
in
{
  # The lid controls blanking instead of an inactivity timer.
  boot.kernelParams = [ "consoleblank=0" ];

  services.logind.settings.Login = {
    HandleLidSwitch = "ignore";
    HandleLidSwitchExternalPower = "ignore";
    HandleLidSwitchDocked = "ignore";
  };

  services.acpid = {
    enable = true;
    lidEventCommands = refreshScreen;
  };

  # Start at boot without a lid event; later restarts serialize state updates.
  systemd.services.predator-lid-screen = {
    description = "Set the Predator screen power from the lid state";
    wantedBy = [ "multi-user.target" ];
    after = [ "acpid.service" "getty@tty1.service" ];
    # Rapid lid events must not exhaust the service start-rate limit.
    startLimitIntervalSec = 0;
    path = [ pkgs.util-linux ];
    serviceConfig = {
      ExecStart = "${pkgs.host-tools}/bin/host-tools lid-screen";
      Type = "oneshot";
      RemainAfterExit = true;
      # A newer lid event can cancel an in-flight update.
      SuccessExitStatus = "SIGTERM";
    };
  };

  powerManagement.resumeCommands = refreshScreen;
}
