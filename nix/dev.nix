{
  config,
  fenix,
  lib,
  pkgs,
  packages,
  ...
}:

let
  inherit (packages) render_daemon testing scripts;
in

{
  languages.c.enable = true;
  languages.rust = {
    enable = builtins.getEnv "DEVENV_RUST" != "0";
    toolchain = fenix.packages.${pkgs.system}.stable;
  };

  packages =
    (lib.attrValues scripts)
    ++ lib.attrValues {
      inherit (pkgs)
        crate2nix
        just
        lcov
        smtp4dev
        oath-toolkit
        clorinde
        ;
      python = (pkgs.python3.withPackages (p: lib.attrValues { inherit (p) httpx pyotp pypdf; }));
    }
    ++ lib.optional (!pkgs.cargo-llvm-cov.meta.broken) pkgs.cargo-llvm-cov
    ++ lib.optionals (pkgs.stdenv.hostPlatform.isDarwin) (
      lib.attrValues { inherit (pkgs.darwin.apple_sdk.frameworks) SystemConfiguration; }
    );

  services.postgres = {
    enable = true;
    package = pkgs.postgresql_17;
    listen_addresses = "127.0.0.1";
    initialScript = ''
      CREATE USER academy SUPERUSER;
      CREATE DATABASE academy OWNER academy;
    '';
  };

  services.redis = {
    enable = true;
    package = pkgs.valkey;
  };

  processes.render_daemon.exec = ''
    ${lib.getExe render_daemon} --port 8001
  '';

  processes.smtp4dev.exec = ''
    ${lib.getExe pkgs.smtp4dev} --smtpport=2525 --imapport=1143 --user=academy=academy --allowremoteconnections- --authenticationrequired
  '';

  processes.testing-recaptcha.exec = ''
    ${lib.getExe testing} recaptcha --port 8100
  '';

  processes.testing-oauth2.exec = ''
    ${lib.getExe testing} oauth2 --port 8101
  '';

  processes.testing-vat.exec = ''
    ${lib.getExe testing} vat --port 8102
  '';

  processes.testing-paypal.exec = ''
    ${lib.getExe testing} paypal --port 8103
  '';

  env = {
    ACADEMY_DEVENV = "1";

    RUST_LOG =
      let
        log = builtins.getEnv "RUST_LOG";
      in
      if log != "" then log else "info,academy=trace";

    PGDATABASE = "academy";

    SMTP4DEV_URL = "http://127.0.0.1:5000";

    PYTHONPATH = "${config.devenv.root}/nix/tests";

    CHROME_BIN = lib.getExe pkgs.ungoogled-chromium;

    ACADEMY_CONFIG = "${config.devenv.root}/config.dev.toml";
  };

  process.manager.implementation = "hivemind";

  scripts = {
    devenv-reset.exec = ''
      rm -rf ${lib.escapeShellArg config.devenv.state}
    '';
  };
}
