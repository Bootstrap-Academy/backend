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

    importNixpkgs = system: import nixpkgs {inherit system;};

    mkDevShell = {
      system,
      root ? null,
    }:
      devenv.lib.mkShell {
        inputs = inputs // {inherit (self.packages.${system}) testing generate update-swagger-ui;};
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
        devenv-up = self.devShells.${system}.default.config.procfileScript;
        devShell = mkDevShell {
          inherit system;
          root = "/fake-root";
        };
      });

    nixosModules = {
      default = import ./nix/module.nix self;
    };

    devShells = eachDefaultSystem (system: {
      default = mkDevShell {inherit system;};
    });

    formatter = eachDefaultSystem (system: (importNixpkgs system).alejandra);

    checks = builtins.mapAttrs (system: packages: builtins.removeAttrs packages (["devenv-up"] ++ (lib.optional (system != "x86_64-linux") "tests"))) self.packages;
  };

  nixConfig = {
    extra-substituters = "https://cache.bootstrap.academy";
    extra-trusted-public-keys = "cache.bootstrap.academy-1:unYr62tCwkIIohOUTXowIvzdqOl+0DlJNfYjEOZxdFE=";
  };
}
