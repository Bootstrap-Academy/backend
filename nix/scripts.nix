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
    in
    pkgs.writeShellScriptBin "generate-clorinde" ''
      set -e

      cd "$(${lib.getExe pkgs.git} rev-parse --show-toplevel)/academy_persistence/postgres"

      static=(Cargo.toml)
      if [[ "$1" != "-f" ]]; then
        mkdir -p .clorinde.bak
        for f in "''${static[@]}"; do mkdir -p "$(dirname ".clorinde.bak/$f")"; cp -f "clorinde/$f" ".clorinde.bak/$f"; done
      fi
      PATH="${rustfmtWrapper}/bin:$PATH" ${lib.getExe pkgs.clorinde} live "postgres://academy@127.0.0.1:5432/academy"
      if [[ "$1" != "-f" ]]; then
        for f in "''${static[@]}"; do cp ".clorinde.bak/$f" "clorinde/$f"; done
        rm -rf .clorinde.bak
      fi
      ${lib.getExe' toolchain.toolchain "cargo"} fmt -p clorinde -- --config-path /dev/null
      ${lib.getExe pkgs.gnused} -i '/use postgres;/d' clorinde/src/lib.rs
      ${lib.getExe pkgs.gnused} -i '/^#\[cfg(feature = "time")\]$/,/^}$/d' clorinde/src/types.rs
      ${lib.getExe pkgs.gnused} -i '/^#\[cfg/d' clorinde/src/{lib,types,client/async_}.rs
      ${lib.getExe pkgs.gnused} -i 's/use fallible_iterator/use postgres::fallible_iterator/' clorinde/src/array_iterator.rs
      ${lib.getExe pkgs.gnused} -i "s/+ 'c/+ use<'c, C, T, N>/" clorinde/src/queries/*.rs
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
