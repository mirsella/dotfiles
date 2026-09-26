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
    enableStrictShellChecks = true;
    path = [ pkgs.util-linux ];
    script = ''
      read -r _ state < /proc/acpi/button/lid/LID0/state
      case "$state" in
        open) blank=poke ;;
        closed) blank=force ;;
        *)
          echo "Unexpected lid state: $state" >&2
          exit 1
          ;;
      esac
      setterm --term linux --blank "$blank" < /dev/tty0 > /dev/tty0
      echo "Lid $state: console blank $blank"
    '';
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      # A newer lid event can cancel an in-flight update.
      SuccessExitStatus = "SIGTERM";
    };
  };

  powerManagement.resumeCommands = refreshScreen;
}
