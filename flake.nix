# New dotfiles repo: NixOS (predator) + standalone Home Manager (Arch hosts).
# Built fresh here, later copied on top of mirsella/dotfiles as a migration commit.
{
  description = "mirsella dotfiles";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    home-manager = {
      url = "github:nix-community/home-manager/release-26.05";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    sops-nix = {
      url = "github:Mic92/sops-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    disko = {
      url = "github:nix-community/disko";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, home-manager, sops-nix, disko, ... }:
    {
      nixosConfigurations.predator = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          ./configuration.nix
          sops-nix.nixosModules.sops
          disko.nixosModules.disko
        ];
      };

      # Standalone Home Manager for Arch hosts lands here next:
      # homeConfigurations."mirsella@laptop", "mirsella@main".
    };
}
