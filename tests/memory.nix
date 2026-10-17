flake:
let
  inherit (flake.inputs.nixpkgs) lib;
  policy = import ../modules/workstation-memory.nix;
  hosts = lib.mapAttrs (_: host: host.config) flake.nixosConfigurations;
in
assert lib.assertMsg (lib.all
  (
    host:
    host.zramSwap.enable
    && host.zramSwap.algorithm == policy.zramSwap.algorithm
    && host.zramSwap.memoryPercent == 200
    && host.zramSwap.priority == policy.zramSwap.priority
    && lib.all (key: host.boot.kernel.sysctl.${key} == policy.boot.kernel.sysctl.${key}) (
      builtins.attrNames policy.boot.kernel.sysctl
    )
    && builtins.elem "zswap.enabled=0" host.boot.kernelParams
  )
  [
    hosts.main
    hosts.laptop
  ]
) "Main and laptop must share the compressed-swap policy";
assert lib.assertMsg (
  !hosts.predator.zramSwap.enable
  && lib.all (key: !(builtins.hasAttr key hosts.predator.boot.kernel.sysctl)) (
    builtins.attrNames policy.boot.kernel.sysctl
  )
) "Workstation memory tuning must not change Predator's server defaults";
true
