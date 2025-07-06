{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-clorinde.url = "github:NixOS/nixpkgs/pull/422700/merge";
    fenix.url = "github:nix-community/fenix";
    devenv = {
      url = "github:cachix/devenv";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      fenix,
      devenv,
      ...
    }@inputs:
    let
      inherit (nixpkgs) lib;

      eachDefaultSystem = lib.genAttrs [
        "x86_64-linux"

        # untested
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      importNixpkgs =
        system:
        import nixpkgs {
          inherit system;
          overlays = [
            (final: prev: { inherit (inputs.nixpkgs-clorinde.legacyPackages.${system}) clorinde; })
          ];
        };

      mkDevShell =
        {
          system,
          root ? null,
        }:
        devenv.lib.mkShell {
          inputs = inputs // {
            packages = self.packages.${system};
          };
          pkgs = importNixpkgs system;
          modules = [
            ./nix/dev.nix
            { devenv.root = lib.mkIf (root != null) root; }
          ];
        };
    in
    {
      packages = eachDefaultSystem (
        system:
        let
          pkgs = importNixpkgs system;
        in
        (pkgs.callPackages ./nix/packages.nix { inherit fenix self; })
        // {
          tests = pkgs.callPackages ./nix/tests { inherit self; };
          scripts = pkgs.callPackages ./nix/scripts.nix { inherit fenix; };
          devenv-up = self.devShells.${system}.default.config.procfileScript;

          checks = pkgs.linkFarm "academy-checks" (
            lib.removeAttrs self.packages.${system} [ "checks" ]
            // rec {
              tests = self.packages.${system}.tests.composite;
              scripts = pkgs.linkFarm "scripts" self.packages.${system}.scripts;
              devShell = mkDevShell {
                inherit system;
                root = "/fake-root";
              };
              devenv-up = devShell.config.procfileScript;
            }
          );
        }
      );

      nixosModules = {
        default = import ./nix/module.nix self;
      };

      devShells = eachDefaultSystem (system: {
        default = mkDevShell { inherit system; };
      });

      formatter = eachDefaultSystem (
        system:
        let
          pkgs = inputs.nixpkgs.legacyPackages.${system};
        in
        pkgs.treefmt.withConfig {
          settings = lib.mkMerge [
            ./treefmt.nix
            { _module.args = { inherit pkgs; }; }
          ];
        }
      );
    };

  nixConfig = {
    extra-substituters = [
      "https://cache.bootstrap.academy/academy"
      "https://nix-community.cachix.org"
      "https://devenv.cachix.org"
    ];
    extra-trusted-public-keys = [
      "academy:JU67oyd32Kzh7XFkUD/rZ6I3wVT8xMtgghwBvEINGus="
      "nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
      "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw="
    ];
  };
}
