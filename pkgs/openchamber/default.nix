{ lib, stdenv, fetchurl, nodejs, python3, jq, makeWrapper }:
stdenv.mkDerivation rec {
  pname = "openchamber";
  version = "1.24.1";

  src = fetchurl {
    url = "https://registry.npmjs.org/@openchamber/web/-/web-${version}.tgz";
    hash = "sha256-T+9bVDR7y3sb46LtudzTCOMWNxLVW0tybGKug9R/YAc=";
  };

  nativeBuildInputs = [
    nodejs
    python3
    jq
    makeWrapper
  ];

  buildPhase = ''
    runHook preBuild
    tar -xzf $src
    cd package
    jq 'del(.scripts.prepack, .scripts.prepare)' package.json > package.json.tmp
    mv package.json.tmp package.json
    export HOME=$TMPDIR npm_config_cache=$TMPDIR/npm-cache
    npm install --no-audit --no-fund --legacy-peer-deps --dangerously-allow-all-scripts
    rm -f package-lock.json
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    mkdir -p $out
    cp -r bin server package.json node_modules $out/
    wrapProgram $out/bin/openchamber --prefix PATH : ${lib.makeBinPath [ nodejs ]}
    runHook postInstall
  '';

  outputHashMode = "recursive";
  outputHashAlgo = "sha256";
  outputHash = lib.fakeHash;

  meta = {
    description = "OpenChamber web server";
    homepage = "https://github.com/openchamber/openchamber";
    license = lib.licenses.mit;
    mainProgram = "openchamber";
  };
}
