{ lib, rustPlatform, fetchgit, pkg-config }:
rustPlatform.buildRustPackage rec {
  pname = "bevy-cli";
  version = "lint-v0.6.0-unstable-2026-10-23";
  src = fetchgit {
    url = "https://github.com/TheBevyFlock/bevy_cli";
    rev = "c69187ccb034ff4d6c2e4e6682bed3c080c119b9";
    hash = "sha256-aJq5zVN99R6LhmiVh4hNJAdlKuiDdaKtPxvuD0363I8=";
  };
  cargoHash = "sha256-pVqa6pPmIRmMgcA7G1rewcdYQHACnRRGudbfKu8Jbtg=";
  nativeBuildInputs = [ pkg-config ];
  doCheck = false;
}
