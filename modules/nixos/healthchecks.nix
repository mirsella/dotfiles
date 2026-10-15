# healthchecks.io dead-man checks: each scheduled job pings its check on a
# successful run via systemd's OnSuccess; a missing ping means the job failed
# or did not run, and healthchecks.io alerts on it.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  checks = [
    "db-backup"
    "recovery-backup"
    "hermes-backup"
    "sanoid"
    "restic-backup"
    "restic-check"
  ];
  secretDir = lib.dirOf config.sops.secrets."healthchecks-${builtins.head checks}".path;
  ping = pkgs.writeShellScript "healthchecks-ping" ''
    set -eu
    exec ${pkgs.curl}/bin/curl --silent --show-error --fail --max-time 15 --retry 3 \
      "$(cat ${secretDir}/healthchecks-$1)"
  '';
in
{
  sops.secrets = lib.genAttrs' checks (key: lib.nameValuePair "healthchecks-${key}" {
    sopsFile = ../../secrets/healthchecks.yaml;
  });

  # Wire a job with: unitConfig.OnSuccess = [ "healthchecks-ping@<key>.service" ];
  # The instance name is the check key and matches the secret's key suffix.
  systemd.services."healthchecks-ping@" = {
    description = "Ping healthchecks.io check %i";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${ping} %i";
    };
  };
}
