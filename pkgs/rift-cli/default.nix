{ lib, rustPlatform, fetchgit, pkg-config }:
rustPlatform.buildRustPackage rec {
  pname = "rift-cli";
  version = "2026.09.19";
  src = fetchgit {
    url = "https://github.com/anomalyco/rift";
    rev = "757a22cb247f9b24a849c9d6bd56f49c0ec494f8";
    hash = "sha256-yr9UVzpo3M3X8ovpTpGEQQ1Zm0G9uuh9tl7VVbWrArs=";
  };
  cargoHash = "sha256-JdIPIun3d5HURgX7m/HOspj5DJoLI6N8C1lmvvDVU4I=";
  nativeBuildInputs = [ pkg-config ];
  doCheck = false;
}
