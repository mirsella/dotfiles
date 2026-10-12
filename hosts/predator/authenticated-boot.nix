{
  config,
  lib,
  pkgs,
  utils,
  ...
}:
let
  enabled = config.boot.initrd.systemd.tpm2.enable;
  cfg = config.boot.lanzaboote;
  disks = import ./disks.nix;
  dataDisks = builtins.removeAttrs disks.luks [ "root" ];
  signature = "/run/credentials/@encrypted/predator-pcr-signature";
  tpmOptions = [
    "tpm2-device=auto"
    "tpm2-pcrs=7"
    "tpm2-signature=${signature}"
    "password-cache=no"
  ];
  unlocks = map (name: "systemd-cryptsetup@${utils.escapeSystemdPath "${name}-crypt"}.service") (
    builtins.attrNames dataDisks
  );
  upstreamInstall = pkgs.writeShellScript "predator-lanzaboote-install" ''
    export PATH="${config.systemd.package}/lib/systemd:$PATH"
    ${cfg.installCommand} \
      --public-key=${lib.escapeShellArg cfg.publicKeyFile} \
      --private-key=${lib.escapeShellArg cfg.privateKeyFile} \
      ${lib.escapeShellArg config.boot.loader.efi.efiSysMountPoint} \
      /nix/var/nix/profiles/system-*-link
  '';
  runtimeConfig = pkgs.writers.writeJSON "predator-boot.json" {
    esp = config.boot.loader.efi.efiSysMountPoint;
    certificate = cfg.publicKeyFile;
    token_directory = "${config.systemd.package}/lib/cryptsetup";
    installer = upstreamInstall;
    recovery_identity = builtins.dirOf config.boot.initrd.secrets."/etc/predator-recovery-ssh.cred";
    disks = disks.luks;
  };
  tool = pkgs.writeShellApplication {
    name = "predator-boot";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.openssl
      pkgs.openssh
      pkgs.cryptsetup
      config.systemd.package
      pkgs.sbsigntool
    ];
    text = ''
      export PATH="${config.systemd.package}/lib/systemd:$PATH"
      exec ${pkgs.host-tools}/bin/host-tools predator-boot --config ${runtimeConfig} "$@"
    '';
  };
in
{
  environment.systemPackages = [ tool ];
  boot.loader.external.installHook = lib.mkForce (
    pkgs.writeShellScript "predator-authenticated-install" ''
      exec ${tool}/bin/predator-boot install
    ''
  );
  boot.lanzaboote.configurationLimit = 5;
  boot.kernelParams = lib.mkIf enabled [ "predator.authenticated-boot=1" ];
  boot.initrd.systemd.tpm2.pcrphases.enable = enabled;
  systemd.tpm2.pcrphases.enable = enabled;
  boot.initrd.luks.devices.crypt-root.crypttabExtraOpts = lib.mkIf enabled (lib.mkForce tpmOptions);
  # Only root unlocks in initrd. Optional data attempts are bounded and parallel.
  environment.etc.crypttab.text = lib.concatStrings (
    lib.mapAttrsToList (
      name: device:
      "${name}-crypt ${device} - ${
        lib.concatStringsSep "," (
          tpmOptions
          ++ lib.optional (name == "fast") "discard"
          ++ [
            "nofail"
            "headless=true"
            "x-systemd.device-timeout=30s"
          ]
        )
      }\n"
    ) dataDisks
  );
  systemd.services."systemd-cryptsetup@" = {
    overrideStrategy = "asDropin";
    restartIfChanged = false;
    stopIfChanged = false;
    serviceConfig = {
      TimeoutSec = "2min";
      Environment = "SYSTEMD_LOG_LEVEL=debug";
    };
  };
  # TPM-disabled study boots must not start measurements without the driver.
  boot.initrd.systemd.services.systemd-pcrosseparator = lib.mkIf enabled {
    description = "TPM PCR OS Separator";
    wantedBy = [ "sysinit.target" ];
    after = [ "tpm2.target" ];
    before = [
      "sysinit.target"
      "shutdown.target"
      "initrd-switch-root.target"
      "cryptsetup-pre.target"
      "cryptsetup.target"
    ];
    conflicts = [
      "shutdown.target"
      "initrd-switch-root.target"
    ];
    unitConfig = {
      DefaultDependencies = false;
      ConditionPathExists = "/etc/initrd-release";
      ConditionSecurity = "measured-os";
      FailureAction = "reboot-force";
    };
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = "${config.boot.initrd.systemd.package}/lib/systemd/systemd-pcrextend --graceful --pcr=0 --pcr=1 --pcr=2 --pcr=3 --pcr=4 --pcr=5 --pcr=6 --pcr=7 --pcr=9 --pcr=12 --pcr=13 --pcr=14 --event-type=os-separator os-separator";
    };
  };
  boot.initrd.systemd.storePaths = lib.mkIf enabled [
    "${config.boot.initrd.systemd.package}/lib/systemd/systemd-pcrextend"
  ];
  boot.initrd.systemd.services."systemd-cryptsetup@crypt\\x2droot" = lib.mkIf enabled {
    overrideStrategy = "asDropin";
    serviceConfig.Environment = "SYSTEMD_LOG_LEVEL=debug";
  };
  # nofail crypttab units are not ordered before cryptsetup.target. Close the
  # data-unlock phase only after all four bounded attempts have completed.
  systemd.services.systemd-pcrphase-sysinit = lib.mkIf enabled {
    wants = unlocks;
    after = unlocks;
  };
}
