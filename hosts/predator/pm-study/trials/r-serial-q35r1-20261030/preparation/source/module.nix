{
  stdenv,
  kernel,
  kernelModuleMakeFlags,
}:
stdenv.mkDerivation {
  pname = "predator-pm-stage";
  version = "20261009";
  src = ./pm_stage.c;
  dontUnpack = true;
  nativeBuildInputs = kernel.moduleBuildDependencies;
  preBuild = ''
    cp $src pm_stage.c
    printf 'obj-m += pm_stage.o\n' > Makefile
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
