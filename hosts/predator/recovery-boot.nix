{
  config,
  lib,
  ...
}:
let
  enabled = config.boot.initrd.systemd.tpm2.enable;
  encrypted = "/etc/predator-recovery-ssh.cred";
  private = "/run/predator-recovery/host-key";
  signature = "/run/credentials/@encrypted/predator-pcr-signature";
in
{
  # Console recovery remains independent of TPM; remote identity does not.
  boot.initrd.availableKernelModules = [ "r8169" ];
  boot.initrd.systemd.network = {
    enable = lib.mkIf enabled true;
    wait-online.enable = false;
    networks."10-predator-recovery" = {
      matchConfig.MACAddress = "98:29:a6:3a:a7:50";
      address = [ "192.168.1.19/24" ];
      networkConfig = {
        DHCP = "no";
        DNS = [ "192.168.1.1" ];
      };
      routes = [ { Gateway = "192.168.1.1"; } ];
    };
  };
  boot.initrd.network.ssh = {
    enable = lib.mkIf enabled true;
    port = 2223;
    authorizedKeys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMXuc6N//8+RfjUhuRZ+COgynjfwFqAeoKAWMUz6s+Pe"
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGkl0CiN6/cLz1OOzBvHaPAMKTnYI0sOlKFDRW25uReF"
    ];
    hostKeys = lib.mkIf enabled (lib.mkDefault [ ]);
    ignoreEmptyHostKeys = lib.mkIf enabled true;
    extraConfig = lib.mkIf enabled "HostKey ${private}";
  };
  # Only ciphertext enters the ESP. Provision it before Lanzaboote appends secrets.
  boot.initrd.secrets.${encrypted} = "/var/lib/predator-boot/recovery-ssh/host-key.cred";
  boot.initrd.systemd.storePaths = lib.mkIf enabled [
    "${config.boot.initrd.systemd.package}/bin/systemd-creds"
    "${config.programs.ssh.package}/bin/ssh-keygen"
  ];
  boot.initrd.systemd.services.predator-recovery-key = lib.mkIf enabled {
    description = "Unseal initrd recovery SSH identity";
    after = [
      "systemd-pcrosseparator.service"
      "systemd-pcrphase-initrd.service"
      "initrd-nixos-copy-secrets.service"
    ];
    requires = [ "systemd-pcrphase-initrd.service" ];
    before = [
      "sshd.service"
      "initrd.target"
      "initrd-switch-root.target"
      "shutdown.target"
    ];
    conflicts = [
      "initrd-switch-root.target"
      "shutdown.target"
    ];
    unitConfig.DefaultDependencies = false;
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      RuntimeDirectory = "predator-recovery";
      RuntimeDirectoryMode = "0700";
      UMask = "0077";
      TimeoutStartSec = "30s";
      ExecStart = "${config.boot.initrd.systemd.package}/bin/systemd-creds decrypt --name=predator-recovery-ssh --refuse-null --no-ask-password --tpm2-signature=${signature} ${encrypted} ${private}";
      ExecStartPost = "${config.programs.ssh.package}/bin/ssh-keygen -lf ${private} -E sha256";
    };
  };
  boot.initrd.systemd.services.sshd = lib.mkIf enabled {
    requires = [ "predator-recovery-key.service" ];
    after = [ "predator-recovery-key.service" ];
    before = [
      "initrd.target"
      "initrd-switch-root.target"
    ];
    conflicts = [ "initrd-switch-root.target" ];
    serviceConfig.KillMode = lib.mkForce "control-group";
  };
  boot.initrd.systemd.extraBin.systemd-reply-password =
    "${config.boot.initrd.systemd.package}/lib/systemd/systemd-reply-password";
  boot.initrd.systemd.extraBin.systemd-tty-ask-password-agent =
    "${config.boot.initrd.systemd.package}/bin/systemd-tty-ask-password-agent";
  systemd.tmpfiles.rules = [ "d /var/lib/predator-boot 0700 root root -" ];
}
