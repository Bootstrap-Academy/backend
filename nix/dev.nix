{
  config,
  fenix,
  lib,
  pkgs,
  testing,
  generate,
  generate-clorinde,
  update-swagger-ui,
  ...
}: {
  languages.c.enable = true;
  languages.rust = {
    enable = builtins.getEnv "DEVENV_RUST" != "0";
    toolchain = fenix.packages.${pkgs.system}.stable;
  };

  packages =
    [generate generate-clorinde update-swagger-ui]
    ++ (with pkgs; [
      crate2nix
      alejandra
      just
      lcov
      smtp4dev
      oath-toolkit
      clorinde
      (python3.withPackages (p: with p; [httpx pyotp pypdf]))
    ])
    ++ (lib.optional (!pkgs.cargo-llvm-cov.meta.broken) pkgs.cargo-llvm-cov)
    ++ (lib.optionals (pkgs.stdenv.hostPlatform.isDarwin) (with pkgs.darwin.apple_sdk.frameworks; [
      SystemConfiguration
    ]));

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

  processes.smtp4dev.exec = ''
    ${pkgs.smtp4dev}/bin/smtp4dev --smtpport=2525 --imapport=1143 --user=academy=academy --allowremoteconnections- --authenticationrequired
  '';

  processes.testing-recaptcha.exec = ''
    ${testing}/bin/academy-testing recaptcha
  '';

  processes.testing-oauth2.exec = ''
    ${testing}/bin/academy-testing oauth2
  '';

  processes.testing-vat.exec = ''
    ${testing}/bin/academy-testing vat
  '';

  processes.testing-paypal.exec = ''
    ${testing}/bin/academy-testing paypal
  '';

  env = {
    ACADEMY_DEVENV = "1";

    RUST_LOG = let
      log = builtins.getEnv "RUST_LOG";
    in
      if log != ""
      then log
      else "info,academy=trace";

    PGDATABASE = "academy";

    SMTP4DEV_URL = "http://127.0.0.1:5000";

    PYTHONPATH = "${config.devenv.root}/nix/tests";

    ACADEMY_CONFIG = let
      chrome = pkgs.ungoogled-chromium;

      renderConfig = (pkgs.formats.toml {}).generate "config.default.toml" {
        render.chrome_bin = lib.getExe chrome;
      };

      configs =
        lib.optional (lib.meta.availableOn {inherit (pkgs) system;} chrome) renderConfig
        ++ [
          "${config.devenv.root}/config.dev.toml"
        ];
    in
      builtins.concatStringsSep ":" configs;
  };

  process.manager.implementation = "hivemind";

  scripts = {
    devenv-reset.exec = ''
      rm -rf ${lib.escapeShellArg config.devenv.state}
    '';
  };
}
