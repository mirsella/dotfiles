{ callPackage, fetchFromGitHub, mesa }:
let
  version = "3.3.0";
  src = fetchFromGitHub {
    owner = "anomalyco";
    repo = "hex";
    tag = "v${version}";
    hash = "sha256-LhtYNTzZ2au14c5bG7v+Ps2KUC9r6mBFtp9mg7FmOOQ=";
  };
in
# Use upstream's native Linux build and runtime wrappers, not an AppImage.
(callPackage "${src}/nix/package.nix" { }).overrideAttrs (previous: {
  inherit version src;
  # Arch's ICDs refer to host libraries that a Nix executable cannot resolve.
  # Supply matching Mesa drivers, while respecting an explicit driver override.
  postFixup = previous.postFixup + ''
    wrapProgram "$out/bin/hex" --set-default VK_DRIVER_FILES "${mesa}/share/vulkan/icd.d"
  '';
})
