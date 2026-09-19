inputs: final: prev: {
  openchamber = final.callPackage ./pkgs/openchamber { };
  zfs-dirty-flag = final.callPackage ./pkgs/zfs-dirty-flag { };
  nushell = inputs.nixpkgs-unstable.legacyPackages.${final.system}.nushell;
  rtk = final.callPackage ./pkgs/rtk { };
  stuff = final.callPackage ./pkgs/stuff { };
  bevy-cli = final.callPackage ./pkgs/bevy-cli { };
  rift-cli = final.callPackage ./pkgs/rift-cli { };
  kache = final.callPackage ./pkgs/kache { };
  computer-use-mcp =
    final.callPackage ./pkgs/computer-use-mcp
      { rustPlatform = inputs.nixpkgs-unstable.legacyPackages.${final.system}.rustPlatform; };
  rioterm =
    final.callPackage ./pkgs/rioterm
      { rustPlatform = inputs.nixpkgs-unstable.legacyPackages.${final.system}.rustPlatform; };
  sfw = final.callPackage ./pkgs/sfw { };
  nodemon = final.callPackage ./pkgs/nodemon { };
}
