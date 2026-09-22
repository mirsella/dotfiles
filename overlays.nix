inputs: final: prev:
let
  unstable = inputs.nixpkgs-unstable.legacyPackages.${final.stdenv.hostPlatform.system};
in
{
  openchamber = final.callPackage ./pkgs/openchamber { };
  nushell = unstable.nushell;
  rtk = final.callPackage ./pkgs/rtk { };
  stuff = final.callPackage ./pkgs/stuff { };
  bevy-cli = final.callPackage ./pkgs/bevy-cli { };
  kache = final.callPackage ./pkgs/kache { };
  computer-use-mcp = final.callPackage ./pkgs/computer-use-mcp { rustPlatform = unstable.rustPlatform; };
  rioterm = final.callPackage ./pkgs/rioterm { rustPlatform = unstable.rustPlatform; };
  sfw = final.callPackage ./pkgs/sfw { };
  nodemon = final.callPackage ./pkgs/nodemon { };
  wtp = final.callPackage ./pkgs/wtp { };
}
