{
  lib,
  pkgs,
  system,
  fenix,
  ...
}: let
  toolchain = fenix.packages.${system}.stable;
in {
  generate = pkgs.writeShellScriptBin "generate" ''
    cd "$(${lib.getExe pkgs.git} rev-parse --show-toplevel)"

    ${lib.getExe pkgs.crate2nix} generate
  '';

  generate-clorinde = pkgs.writeShellScriptBin "generate-clorinde" ''
    set -e

    cd "$(${lib.getExe pkgs.git} rev-parse --show-toplevel)/academy_persistence/postgres"

    static=(Cargo.toml)
    if [[ "$1" != "-f" ]]; then
      mkdir -p .clorinde.bak
      for f in "''${static[@]}"; do mkdir -p "$(dirname ".clorinde.bak/$f")"; cp -f "clorinde/$f" ".clorinde.bak/$f"; done
    fi
    ${lib.getExe pkgs.clorinde} live "postgres://academy@127.0.0.1:5432/academy"
    if [[ "$1" != "-f" ]]; then
      for f in "''${static[@]}"; do cp ".clorinde.bak/$f" "clorinde/$f"; done
      rm -rf .clorinde.bak
    fi
    ${lib.getExe' toolchain.toolchain "cargo"} fmt -p clorinde -- --config-path /dev/null
    ${lib.getExe pkgs.gnused} -i '/use postgres;/d' clorinde/src/lib.rs
    ${lib.getExe pkgs.gnused} -i '/^#\[cfg(feature = "time")\]$/,/^}$/d' clorinde/src/types.rs
    ${lib.getExe pkgs.gnused} -i '/^#\[cfg/d' clorinde/src/{lib,types,client/async_}.rs
  '';

  update-swagger-ui = pkgs.writeShellScriptBin "update-swagger-ui" ''
    export PATH=${lib.escapeShellArg (lib.makeBinPath (with pkgs; [git coreutils curl jq gnutar gzip]))}

    cd "$(git rev-parse --show-toplevel)/academy_assets/assets/swagger-ui"

    url=$(curl https://api.github.com/repos/swagger-api/swagger-ui/releases/latest | jq -r .tarball_url)
    curl -L "$url" | tar xvz --wildcards --no-wildcards-match-slash '*/dist'
    mv swagger-api-swagger-ui-*/dist/{swagger-ui-bundle.js,swagger-ui.css} .
    rm -rf swagger-api-swagger-ui-*
  '';
}
