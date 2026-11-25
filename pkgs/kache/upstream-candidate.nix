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
  version = "2.1.1";
  src = fetchFromGitHub {
    owner = "kunobi-ninja";
    repo = "kache";
    rev = "3aad1a1974efcae8c8827a7200d5cf7cd270319e";
    hash = "sha256-qCv7ierYlx9dZhea+SRMxsIRRq4LFFTzKNoiKOZ6448=";
  };
  cargoLock.lockFile = "${src}/Cargo.lock";
  patches = [
    ./upstream-candidate.patch
    ./compile-first-observations.patch
    ./retention-shadow.patch
    ./case-analytics.patch
  ];
  postPatch = ''
    substituteInPlace crates/kache-ipc/src/execution/unix_client.rs \
      --replace-fail '/bin/cat' '${coreutils}/bin/cat'
  '';
  KACHE_VERSION = "${version}+local-case-analytics-20261125";
  CARGO_PROFILE_RELEASE_LTO = "thin";
  nativeBuildInputs = [ pkg-config ];
  nativeCheckInputs = [ git cacert bashInteractive zsh ];
  KACHE_TEST_SLEEP = "${coreutils}/bin/sleep";
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
  '';
  meta = {
    description = "Content-addressed build cache for Rust and C/C++";
    homepage = "https://github.com/kunobi-ninja/kache";
    license = lib.licenses.asl20;
    mainProgram = "kache";
  };
}
