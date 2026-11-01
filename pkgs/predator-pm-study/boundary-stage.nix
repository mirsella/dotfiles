{ stdenv, kernel, kernelModuleMakeFlags }:
stdenv.mkDerivation {
  pname = "predator-pm-boundary-stage";
  version = "20261101";
  src = ./boundary-stage.c;
  dontUnpack = true;
  nativeBuildInputs = kernel.moduleBuildDependencies;
  preBuild = ''
    cp $src pm_boundary_stage.c
    printf 'obj-m += pm_boundary_stage.o\n' > Makefile
  '';
  makeFlags = kernelModuleMakeFlags ++ [
    "-C" "${kernel.dev}/lib/modules/${kernel.modDirVersion}/build"
    "M=$(PWD)" "INSTALL_MOD_PATH=${placeholder "out"}" "INSTALL_MOD_DIR=extra"
  ];
  buildTargets = [ "modules" ];
  installTargets = [ "modules_install" ];
}
