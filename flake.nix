{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
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
            clorinde = final.rustPlatform.buildRustPackage rec {
              pname = "clorinde";
              version = "0.11.1-unstable-2025-01-23";

              src = final.fetchFromGitHub {
                owner = "halcyonnouveau";
                repo = "clorinde";
                rev = "d111bbfd49062d459c81ae529d1ebf5f6c7275e6";
                hash = "sha256-nI3IvSxQ6bZ72GrBVYDCI0NQT8s/H19P5WDCGNF3cCI=";
              };

              cargoHash = "sha256-f4vXITZxWXgozpj53HBZIL/w9becyZBvXc05KSYEjZI=";

              cargoBuildFlags = ["--package=clorinde"];
              cargoTestFlags = cargoBuildFlags;

              nativeInstallCheckInputs = [final.versionCheckHook];
              preVersionCheck = ''
                export version=${lib.head (lib.split "-" version)}
              '';
              versionCheckProgramArg = "--version";
              doInstallCheck = true;

              meta.mainProgram = "clorinde";
            };
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
