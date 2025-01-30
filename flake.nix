{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-clorinde.url = "github:NixOS/nixpkgs/pull/377847/merge";
    fenix.url = "github:nix-community/fenix";
    devenv = {
      url = "github:cachix/devenv";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    fenix,
    devenv,
    ...
  } @ inputs: let
    inherit (nixpkgs) lib;

    eachDefaultSystem = lib.genAttrs [
      "x86_64-linux"

      # untested
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];

    importNixpkgs = system:
      import nixpkgs {
        inherit system;
        overlays = [
          (final: prev: {
            clorinde = assert prev.clorinde.version == "0.11.2";
              inputs.nixpkgs-clorinde.legacyPackages.${system}.clorinde;
          })
        ];
      };

    mkDevShell = {
      system,
      root ? null,
    }:
      devenv.lib.mkShell {
        inputs = inputs // {inherit (self.packages.${system}) testing scripts;};
        pkgs = importNixpkgs system;
        modules = [
          ./nix/dev.nix
          {devenv.root = lib.mkIf (root != null) root;}
        ];
      };
  in {
    packages = eachDefaultSystem (system: let
      pkgs = importNixpkgs system;
    in
      (pkgs.callPackages ./nix/packages.nix {inherit fenix self;})
      // {
        tests = pkgs.callPackages ./nix/tests {inherit self;};
        scripts = pkgs.callPackages ./nix/scripts.nix {inherit fenix;};
        devenv-up = self.devShells.${system}.default.config.procfileScript;

        checks = pkgs.linkFarm "academy-checks" (lib.removeAttrs self.packages.${system} ["checks"]
          // rec {
            tests = self.packages.${system}.tests.composite;
            scripts = pkgs.linkFarm "scripts" self.packages.${system}.scripts;
            devShell = mkDevShell {
              inherit system;
              root = "/fake-root";
            };
            devenv-up = devShell.config.procfileScript;
          });
      });

    nixosModules = {
      default = import ./nix/module.nix self;
    };

    devShells = eachDefaultSystem (system: {
      default = mkDevShell {inherit system;};
    });

    formatter = eachDefaultSystem (system: (importNixpkgs system).alejandra);
  };

  nixConfig = {
    extra-substituters = "https://cache.bootstrap.academy/academy";
    extra-trusted-public-keys = "academy:JU67oyd32Kzh7XFkUD/rZ6I3wVT8xMtgghwBvEINGus=";
  };
}
