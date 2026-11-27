{
  lib,
  rustPlatform,
  fetchFromGitHub,
  pkg-config,
  git,
  cacert,
  coreutils,
  bashInteractive,
  zsh,
}:
rustPlatform.buildRustPackage rec {
  pname = "kache-upstream-candidate";
  version = "2.4.0";
  src = fetchFromGitHub {
    owner = "kunobi-ninja";
    repo = "kache";
    rev = "b64f9a4df361e1548180b5a39cc6effa528a623f";
    hash = "sha256-nYZXAqpcitanRuD4uKcNzjrB03wWE4x9VGoiZTVyER4=";
  };
  cargoLock.lockFile = "${src}/Cargo.lock";
  patches = [ ./build-id-telemetry.patch ];
  postPatch = ''
    substituteInPlace crates/kache-ipc/src/execution/unix_client.rs \
      --replace-fail '/bin/cat' '${coreutils}/bin/cat'
  '';
  KACHE_VERSION = "${version}+local-build-id";
  CARGO_PROFILE_RELEASE_LTO = "thin";
  nativeBuildInputs = [ pkg-config ];
  nativeCheckInputs = [ git cacert bashInteractive zsh ];
  KACHE_TEST_SLEEP = "${coreutils}/bin/sleep";
  RUSTC_WRAPPER = "";
  doCheck = true;
  checkType = "debug";
  checkFlags = [ "--test-threads=2" ];
  cargoTestFlags = [ "--workspace" "--lib" "--bins" ];
  preCheck = ''
    export HOME="$TMPDIR/kache-test-home"
    export KACHE_CACHE_DIR="$TMPDIR/kache-test-cache"
    export SSL_CERT_FILE="${cacert}/etc/ssl/certs/ca-bundle.crt"
    export RUST_TEST_THREADS=2
    export PATH="${bashInteractive}/bin:$PATH"
    mkdir -p "$HOME" "$KACHE_CACHE_DIR"
  '';
  # Ship the split engine and every alias Home Manager installs.
  postInstall = ''
    test -x "$out/bin/kache"
    test -x "$out/bin/kache-engine"
    test -x "$out/bin/kache-clang-cl"
    test -x "$out/bin/kache-cl"
    mkdir -p "$out/lib/kache"
    for name in cc c++ gcc g++ clang clang++ cargo rustdoc; do
      ln -s "$out/bin/kache" "$out/lib/kache/$name"
    done
    touch "$out/lib/kache/.kache-shims"
    ln -s lib/kache "$out/shims"
  '';
  meta = {
    description = "Content-addressed build cache for Rust and C/C++";
    homepage = "https://github.com/kunobi-ninja/kache";
    license = lib.licenses.asl20;
    mainProgram = "kache";
  };
}
