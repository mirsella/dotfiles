{ stdenv, kernel }:
stdenv.mkDerivation {
  pname = "predator-pm-stage";
  version = "20261009";
  src = ./pm_stage.c;
  dontUnpack = true;
  nativeBuildInputs = kernel.moduleBuildDependencies;
  buildPhase = ''
    cp $src pm_stage.c
    printf 'obj-m += pm_stage.o\n' > Makefile
    make -C ${kernel.dev}/lib/modules/${kernel.modDirVersion}/build M=$PWD modules
  '';
  installPhase = ''
    install -D pm_stage.ko $out/lib/modules/${kernel.modDirVersion}/extra/pm_stage.ko
  '';
}
