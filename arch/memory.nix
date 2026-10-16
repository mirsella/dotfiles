{ pkgs, lib }:
let
  policy = import ../modules/workstation-memory.nix;
  files = {
    "etc/systemd/zram-generator.conf" = ''
      [zram0]
      zram-size = ram * ${toString policy.zramSwap.memoryPercent} / 100
      compression-algorithm = ${policy.zramSwap.algorithm}
      swap-priority = ${toString policy.zramSwap.priority}
      fs-type = swap
    '';
    "etc/sysctl.d/99-swappiness.conf" = lib.generators.toKeyValue { } policy.boot.kernel.sysctl;
  };
in
pkgs.runCommand "workstation-memory-config" { } (
  lib.concatStringsSep "\n" (
    lib.mapAttrsToList (path: text: ''
      install -Dm644 ${pkgs.writeText (builtins.baseNameOf path) text} "$out/${path}"
    '') files
  )
)
