{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-chromium.url = "github:NixOS/nixpkgs/e3e32b642a31e6714ec1b712de8c91a3352ce7e1";
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

      importNixpkgs = system: import nixpkgs { inherit system; };

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
          settings = [
            ./treefmt.nix
            { _module.args = { inherit pkgs; }; }
          ];
        }
      );
    };

  nixConfig = {
    extra-substituters = "https://cache.bootstrap.academy/academy";
    extra-trusted-public-keys = "academy:JU67oyd32Kzh7XFkUD/rZ6I3wVT8xMtgghwBvEINGus=";
  };
}
