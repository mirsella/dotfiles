{ lib, rustPlatform, fetchFromGitHub, pkg-config, git, cacert, enableWorkloadScheduler ? false }:
rustPlatform.buildRustPackage rec {
  pname = "kache";
  version = "1.2.2";
  src = fetchFromGitHub {
    owner = "kunobi-ninja";
    repo = "kache";
    tag = "v${version}";
    hash = "sha256-thzKe50UE6CKYYfKNrVyNdgQdAqr0bLRYOZzvWwlxW8=";
  };
  cargoHash = "sha256-wgBi4CqPgab15O6y0n0re2aZ5QV02LbIlktQVlD8rVQ=";
  # Every event identifies this local recipe without hashing the binary on hits.
  KACHE_VERSION = "${version}+local-reuse-20261109${lib.optionalString enableWorkloadScheduler "-scheduler-trial"}";
  # Keep the stable engine; evaluate scheduler/2.0 changes separately.
  # README.md records upstream provenance and regression coverage.
  patches = [
    ./local-cache-fixes.patch
    ./failure-diagnostics.patch
    ./admission-diagnostics.patch
    # Native/Git inputs, including ownership and timestamp prerequisites.
    ./build-script-correctness.patch
  ] ++ lib.optional enableWorkloadScheduler ./scheduler-workload.patch ++ [
    # Equivalent generated-output profiles share artifacts; rejected discovery
    # predictions release their flight before another checkout compiles.
    ./reuse-sharing.patch
  ];
  nativeBuildInputs = [ pkg-config ];
  nativeCheckInputs = [ git cacert ];
  doCheck = true;
  # The release binary retains upstream fat LTO. Avoid fat-LTO linking the
  # thousands of unit tests as well; exercise the full debug test profile.
  checkType = "debug";
  checkFlags = [ "--test-threads=2" ];
  cargoTestFlags = [ "--workspace" "--lib" "--bins" ];
  preCheck = ''
    # Keep filesystem/deadline tests from saturating a shared build host.
    export RUST_TEST_THREADS=2
    export HOME="$TMPDIR/kache-test-home"
    mkdir -p "$HOME"
    export SSL_CERT_FILE="${cacert}/etc/ssl/certs/ca-bundle.crt"
    export NIX_SSL_CERT_FILE="$SSL_CERT_FILE"
    export KACHE_HOST_CONFIG=
    export KACHE_CACHE_DIR="$TMPDIR/kache-test-cache"
  '';
  meta = {
    description = "Zero-copy, content-addressed build cache for Rust and C/C++";
    homepage = "https://kunobi.ninja/kache";
    license = lib.licenses.asl20;
    mainProgram = "kache";
  };
}
