{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "t3-cache-warmer";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;
  meta = {
    description = "Keep prompt caches warm on active T3 Code threads";
    mainProgram = "t3-cache-warmer";
    platforms = lib.platforms.linux;
  };
}
