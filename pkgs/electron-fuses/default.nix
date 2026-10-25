{ lib, stdenvNoCC, fetchurl, makeWrapper, nodejs }:
stdenvNoCC.mkDerivation rec {
  pname = "electron-fuses";
  version = "2.1.3";

  src = fetchurl {
    url = "https://registry.npmjs.org/@electron/fuses/-/fuses-${version}.tgz";
    hash = "sha256-CuYL3Lue8KEjUIE05VxiS7R3d0p1KPsA+uMeUuVB6cc=";
  };

  nativeBuildInputs = [ makeWrapper ];

  installPhase = ''
    runHook preInstall
    packageDir="$out/lib/node_modules/@electron/fuses"
    install -Dm644 -t "$packageDir" package.json LICENSE
    cp -r dist "$packageDir/dist"
    makeWrapper ${nodejs}/bin/node "$out/bin/electron-fuses" \
      --add-flags "$packageDir/dist/bin.js"
    runHook postInstall
  '';

  meta = {
    description = "Read and write Electron fuses";
    homepage = "https://github.com/electron/fuses";
    license = lib.licenses.mit;
    platforms = lib.platforms.all;
  };
}
