# Evaluate only; never build, activate, mount, or format a disk.
# nix eval --impure --json --file tests/invariants.nix
let
  flake = builtins.getFlake "path:${toString ../.}";
  inherit (flake.inputs.nixpkgs) lib;
  server = flake.nixosConfigurations.predator.config;
  layout = import ../disko.nix;
  disks = layout.disko.devices.disk;
  ncdata = layout.disko.devices.zpool.fast.datasets.ncdata;
  pkgs = flake.inputs.nixpkgs.legacyPackages.x86_64-linux;
  disko = import flake.inputs.disko { inherit lib; };
in
assert lib.assertMsg (
  server.boot.lanzaboote.enable
  && server.boot.initrd.systemd.enable
  && server.boot.initrd.secrets == { }
  && lib.all (d: d.keyFile == null && builtins.elem "tpm2-device=auto" d.crypttabExtraOpts)
    (builtins.attrValues server.boot.initrd.luks.devices)
  && lib.all (fs: !fs.autoFormat) (builtins.attrValues server.fileSystems)
) "Installed system must use signed TPM boot without embedded keys or automatic formatting";
assert lib.assertMsg (
  builtins.elem "tank-unlock.service" server.systemd.services.zfs-import-tank.requires
  && builtins.elem "tank-unlock.service" server.systemd.services.zfs-import-tank.after
  && server.systemd.services.tank-unlock.wantedBy == [ ]
) "The native ZFS import service must require successful HDD unlocking";
assert lib.assertMsg (
  "${disks.ssd.device}-part1" == server.boot.initrd.luks.devices.fast-crypt.device
  && ncdata.options.mountpoint == "legacy"
  && server.fileSystems.${ncdata.mountpoint}.device == "fast/ncdata"
  && lib.all (d:
    let luks = d.content.partitions.crypt.content;
    in luks.askPassword && !luks.initrdUnlock && !(luks.settings ? keyFile)
  ) (builtins.attrValues disks)
) "Fresh provisioning must match runtime mounts and leave credential enrollment explicit";
{
  inherit (server.system.build.toplevel) drvPath;
  formatter = (disko._cliDestroyFormatMount layout pkgs).drvPath;
  workstations = builtins.mapAttrs (_: home:
    let unit = home.config.systemd.user.services.rclone-nextcloud;
    in assert lib.assertMsg (
      builtins.elem "sops-nix.service" unit.Unit.After
      && builtins.elem "sops-nix.service" unit.Unit.Wants
      && !(builtins.elem "sops-nix.service" (unit.Unit.Requires or [ ]))
      && unit.Service.Type == "notify"
      && !(unit.Service ? ExecStop)
      && unit.Service.SuccessExitStatus == "143"
    ) "WebDAV must wait for credentials and let rclone own mount readiness and shutdown";
    home.activationPackage.drvPath
  ) flake.homeConfigurations;
}
