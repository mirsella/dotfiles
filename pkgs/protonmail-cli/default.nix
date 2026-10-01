{ lib, rustPlatform, fetchgit }:
rustPlatform.buildRustPackage {
  pname = "protonmail-cli";
  version = "0.1.2-unstable-2026-10-01";
  src = fetchgit {
    url = "https://github.com/filippofinke/protonmail-rs";
    rev = "3d3c5f562a67ef9e770ebd7f79173858b468db19";
    hash = "sha256-PF1JtpVFXrwO+BOrffrvbupwD/D+cX11DcNJIvFRZpQ=";
  };
  cargoHash = "sha256-VZzI/Xvxu4DTZXNw6RXBHd5p56MdQmRhb0qwynmQnNo=";
  # Only the CLI, not the MCP server.
  cargoBuildFlags = [ "--bin" "protonmail-cli" ];
  doCheck = false; # tests hit the live Proton API and need credentials
  # Unmerged upstream PR #3 (HV/CAPTCHA + TOTP login flow). Code-only hunks;
  # CHANGELOG/README/docs hunks excluded. Drop when merged upstream.
  patches = [ ./pr-3-auth-hv.patch ];
  # proton-crypto pins proton-rpgp/proton-srp to Proton's own registry, which
  # buildRustPackage's offline vendor cannot see. Both crates are published on
  # crates.io at the locked versions, so redirect them there.
  postPatch = ''
    cat >> Cargo.toml <<EOF

    [patch."sparse+https://rust-registry.proton.me/index/"]
    proton-rpgp = { version = "=0.3.1" }
    proton-srp = { version = "=0.8.2" }
    gopenpgp-sys = { version = "=0.3.7" }
    EOF
  '';

  meta = {
    description = "Unofficial Proton Mail CLI client written in Rust";
    homepage = "https://github.com/filippofinke/protonmail-rs";
    license = lib.licenses.mit;
    mainProgram = "protonmail-cli";
  };
}
