# Patched acer-wmi with Predator PH317-51 quirk (hwmon sensors +
# acer-wmi::kbd_backlight LED). Built out-of-tree against the system kernel;
# upstream submission pending. The file is deliberately named
# acer-wmi-ph31751.ko (not acer-wmi.ko): the NixOS modules tree symlinks
# extra/ and kernel/ dirs, which depmod does not descend into, so a
# same-name override would lose to the in-tree module. A unique name loads
# fine and nothing auto-loads the in-tree one (no modalias).
{ config, lib, ... }:
{
  options.hardware.predator-acer-wmi.enable = lib.mkOption {
    type = lib.types.bool;
    default = true;
    description = "Build and load the custom PH317-51 Acer WMI driver.";
  };

  config = lib.mkIf config.hardware.predator-acer-wmi.enable {
    boot.extraModulePackages = [
      (config.boot.kernelPackages.callPackage
        (
          {
            stdenv,
            kernel,
            kernelModuleMakeFlags,
          }:
          stdenv.mkDerivation {
            pname = "acer-wmi-ph31751";
            version = "0.1";
            src = ./acer-wmi-ph31751.c;
            dontUnpack = true;
            nativeBuildInputs = kernel.moduleBuildDependencies;
            preBuild = ''
              cp $src acer-wmi-ph31751.c
              echo "obj-m += acer-wmi-ph31751.o" > Makefile
            '';
            makeFlags = kernelModuleMakeFlags ++ [
              "-C"
              "${kernel.dev}/lib/modules/${kernel.modDirVersion}/build"
              "M=$(PWD)"
              "INSTALL_MOD_PATH=${placeholder "out"}"
              "INSTALL_MOD_DIR=extra"
            ];
            buildTargets = [ "modules" ];
            installTargets = [ "modules_install" ];
          }
        )
        { })
    ];

    # Load our module at boot (nothing auto-loads in-tree acer-wmi, it has no
    # modalias), and redirect manual `modprobe acer-wmi` to it as well.
    boot.kernelModules = [ "acer-wmi-ph31751" ];
    boot.extraModprobeConfig = ''
      alias acer-wmi acer-wmi-ph31751
    '';
  };
}
