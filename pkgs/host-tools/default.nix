{
  lib,
  rustPlatform,
  openssl,
  makeWrapper,
}:
rustPlatform.buildRustPackage {
  pname = "host-tools";
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
  nativeBuildInputs = [ makeWrapper ];
  nativeCheckInputs = [ openssl ];
  postInstall = ''
    makeWrapper "$out/bin/host-tools" "$out/bin/composio-mcp-headers" \
      --add-flags "composio-mcp-headers"
  '';
  meta = {
    description = "Recurring host maintenance and monitoring helpers";
    mainProgram = "host-tools";
    platforms = lib.platforms.linux;
  };
}
