{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "claude-credit-router";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [ ./Cargo.toml ./Cargo.lock ./src ./tests ];
  };
  cargoLock.lockFile = ./Cargo.lock;
  meta = {
    description = "Private API credit routing and usage audit for OpenCode";
    mainProgram = "claude-credit-router";
    platforms = lib.platforms.linux;
  };
}
