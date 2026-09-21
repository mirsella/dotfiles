{
  description = "mirsella dotfiles";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    neovim-nightly-overlay.url = "github:nix-community/neovim-nightly-overlay";
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
    lanzaboote = {
      url = "github:nix-community/lanzaboote/v1.1.0";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, home-manager, sops-nix, disko, lanzaboote, ... }@inputs:
    let
      overlays = [
        inputs.neovim-nightly-overlay.overlays.default
        (import ./overlays.nix inputs)
      ];
      mkStandaloneHome =
        hostName: gitSigningKey:
        home-manager.lib.homeManagerConfiguration {
          pkgs = import nixpkgs {
            system = "x86_64-linux";
            inherit overlays;
          };
          extraSpecialArgs = {
            inherit inputs gitSigningKey hostName;
            isNixOS = false;
          };
          modules = [ ./hosts/arch.nix ];
        };
    in
    {
      nixosConfigurations.predator = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = { inherit inputs; };
        modules = [
          ./configuration.nix
          home-manager.nixosModules.home-manager
          sops-nix.nixosModules.sops
          disko.nixosModules.disko
          lanzaboote.nixosModules.lanzaboote
        ];
      };

      nixosConfigurations.predator-install = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = { inherit inputs; };
        modules = [
          ./hosts/predator-install.nix
          disko.nixosModules.disko
        ];
      };

      homeConfigurations = {
        laptop = mkStandaloneHome "laptop" "E88ECCA3AA187BC1";
        main = mkStandaloneHome "main" "E53202A06B2614A4";
      };
    };
}
