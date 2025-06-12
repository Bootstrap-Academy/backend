{
  fenix,
  callPackage,
  installShellFiles,
  lib,
  pkgs,
  system,
  self,
  stdenv,
  makeWrapper,
  versionCheckHook,
}:

let
  toolchain = fenix.packages.${system}.stable;

  version =
    let
      year = builtins.substring 0 4 self.sourceInfo.lastModifiedDate;
      month = builtins.substring 4 2 self.sourceInfo.lastModifiedDate;
      day = builtins.substring 6 2 self.sourceInfo.lastModifiedDate;
      rev = self.sourceInfo.shortRev or self.sourceInfo.dirtyShortRev;
    in
    "${year}.${month}.${day}+${rev}";

  setVersion =
    drv:
    stdenv.mkDerivation {
      inherit (drv) pname;
      inherit version;
      src = drv;
      nativeBuildInputs = [ makeWrapper ];
      installPhase = ''
        cp -r . $out
        for bin in $out/bin/*; do
          wrapProgram $bin --argv0 $(basename $bin) --set ACADEMY_VERSION ${lib.escapeShellArg version}
        done
      '';
      nativeInstallCheckInputs = [ versionCheckHook ];
      versionCheckProgramArg = "--version";
      doInstallCheck = true;
      passthru = (drv.passthru or { }) // {
        unwrapped = drv;
      };
      meta = { inherit (drv.meta) mainProgram; };
    };

  crateDirs = lib.pipe ../. [
    builtins.readDir
    builtins.attrNames
    (builtins.filter (lib.hasPrefix "academy"))
    (map (x: ../${x}))
  ];

  workspaceMembers = lib.pipe crateDirs [
    (map lib.filesystem.listFilesRecursive)
    lib.flatten
    (builtins.filter (x: baseNameOf x == "Cargo.toml"))
    (map (
      lib.flip lib.pipe [
        builtins.readFile
        fromTOML
        (x: x.package.name)
      ]
    ))
  ];

  cargoToml = fromTOML (builtins.readFile ../Cargo.toml);

  mergeOverrides =
    a: b: attrs:
    (a attrs) // (b (attrs // (a attrs)));
  mergeOverrideSets =
    a: b: a // b // (builtins.mapAttrs (k: _: mergeOverrides a.${k} b.${k}) (lib.intersectAttrs a b));

  defaultOverrides = lib.genAttrs workspaceMembers (
    crate: attrs: {
      preBuild = ''
        ${attrs.preBuild or ""}
        export CARGO_PKG_HOMEPAGE=${lib.escapeShellArg cargoToml.workspace.package.homepage}
        export CARGO_PKG_REPOSITORY=${lib.escapeShellArg cargoToml.workspace.package.repository}
      '';
    }
  );

  binOverride = bin: attrs: {
    pname = bin;
    name = "${bin}-${attrs.version}";
    CARGO_BIN_NAME = bin;
    nativeBuildInputs = attrs.nativeBuildInputs or [ ] ++ [ installShellFiles ];
    buildInputs =
      attrs.buildInputs or [ ]
      ++ lib.optionals stdenv.hostPlatform.isDarwin (
        lib.attrValues { inherit (pkgs.darwin.apple_sdk.frameworks) SystemConfiguration; }
      );
    postInstall = ''
      ${attrs.postInstall or ""}
      installShellCompletion --cmd ${bin} \
        --bash <(COMPLETE=bash $out/bin/${bin}) \
        --fish <(COMPLETE=fish $out/bin/${bin}) \
        --zsh <(COMPLETE=zsh $out/bin/${bin})
    '';
    meta.mainProgram = bin;
  };

  crateOverrides = mergeOverrideSets defaultOverrides {
    academy = binOverride "academy";
    academy_render_daemon = binOverride "academy-render-daemon";
    academy_testing = binOverride "academy-testing";
    academy_assets = attrs: {
      patchPhase = ''
        sed -i 's|env!("CARGO_MANIFEST_DIR"), "/../config.toml"|"${../config.toml}"|' src/lib.rs
        ${attrs.patchPhase or ""}
      '';
    };
    academy_data = _: {
      src = lib.fileset.toSource {
        root = ../academy_data;
        fileset = ../academy_data/src;
      };
    };
  };

  cargoNix = callPackage ../Cargo.nix {
    pkgs = pkgs.extend (
      final: prev: {
        inherit (toolchain) cargo;
        # workaround for https://github.com/NixOS/nixpkgs/blob/d80a3129b239f8ffb9015473c59b09ac585b378b/pkgs/build-support/rust/build-rust-crate/default.nix#L19-L23
        rustc = toolchain.rustc // {
          unwrapped.configureFlags = [ "--target=" ];
        };
      }
    );
    defaultCrateOverrides = mergeOverrideSets pkgs.defaultCrateOverrides crateOverrides;
  };
in

builtins.mapAttrs (_: setVersion) {
  default = cargoNix.workspaceMembers.academy.build;
  render_daemon = cargoNix.workspaceMembers.academy_render_daemon.build.overrideAttrs {
    passthru.chrome = pkgs.ungoogled-chromium;
  };
  testing = cargoNix.workspaceMembers.academy_testing.build;
}
