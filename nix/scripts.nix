{
  lib,
  pkgs,
  system,
  fenix,
  ...
}:
let
  toolchain = fenix.packages.${system}.stable;
in
{
  generate = pkgs.writeShellScriptBin "generate" ''
    cd "$(${lib.getExe pkgs.git} rev-parse --show-toplevel)"

    ${lib.getExe pkgs.crate2nix} generate
  '';

  generate-clorinde =
    let
      rustfmtWrapper =
        pkgs.runCommandNoCC "rustfmt-wrapper" { nativeBuildInputs = [ pkgs.makeWrapper ]; }
          ''
            makeWrapper ${lib.getExe' toolchain.toolchain "rustfmt"} $out/bin/rustfmt --add-flags --config-path=/dev/null
          '';
      runtimeDependencies = lib.attrValues {
        inherit (pkgs)
          coreutils
          gnused
          git
          clorinde
          ;
        inherit (toolchain) toolchain;
      };
    in
    pkgs.writeShellScriptBin "generate-clorinde" ''
      export PATH=${lib.makeBinPath runtimeDependencies}

      set -e

      cd "$(git rev-parse --show-toplevel)/academy_persistence/postgres"

      PATH="${rustfmtWrapper}/bin:$PATH" clorinde live "postgres://academy@127.0.0.1:5432/academy"

      if [[ "$1" != "-f" ]]; then
        git restore clorinde/{.gitattributes,Cargo.toml}
      fi

      cargo fmt -p clorinde -- --config-path /dev/null

      sed -i "s/+ 'c/+ use<'c, C, T, N>/" clorinde/src/queries/*.rs

      sed -i -E '/^#\[cfg/d' clorinde/src/lib.rs
      sed -i -E '/^pub use deadpool_postgres;$/d' clorinde/src/lib.rs

      sed -i -E '/^#\[cfg\(feature = "deadpool"\)\]$/d' clorinde/src/client/async_.rs
      sed -i -E '/^mod deadpool;$/d' clorinde/src/client/async_.rs
      rm clorinde/src/client/async_/deadpool.rs

      sed -i -E 's/^use fallible_iterator\b/use postgres::fallible_iterator/' clorinde/src/array_iterator.rs
    '';

  update-swagger-ui =
    let
      runtimeDependencies = lib.attrValues {
        inherit (pkgs)
          git
          coreutils
          curl
          jq
          gnutar
          gzip
          ;
      };
    in
    pkgs.writeShellScriptBin "update-swagger-ui" ''
      export PATH=${lib.makeBinPath runtimeDependencies}

      cd "$(git rev-parse --show-toplevel)/academy_assets/assets/swagger-ui"

      url=$(curl https://api.github.com/repos/swagger-api/swagger-ui/releases/latest | jq -r .tarball_url)
      curl -L "$url" | tar xvz --wildcards --no-wildcards-match-slash '*/dist'
      mv swagger-api-swagger-ui-*/dist/{swagger-ui-bundle.js,swagger-ui.css} .
      rm -rf swagger-api-swagger-ui-*
    '';
}
