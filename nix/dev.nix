{
  config,
  fenix,
  lib,
  pkgs,
  testing,
  ...
}: {
  languages.rust = {
    enable = builtins.getEnv "DEVENV_RUST" != "0";
    toolchain = fenix.packages.${pkgs.system}.stable;
  };

  packages = with pkgs; [
    alejandra
    just
    cargo-llvm-cov
    lcov
    smtp4dev
    oath-toolkit
    (python3.withPackages (p: with p; [httpx pyotp]))
  ];

  services.postgres = {
    enable = true;
    package = pkgs.postgresql_16;
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

    PYTHONPATH = "${config.env.DEVENV_ROOT}/nix/tests";

    ACADEMY_CONFIG = "${config.devenv.root}/config.dev.toml";
  };

  process.manager.implementation = "hivemind";

  scripts = {
    devenv-reset.exec = ''
      rm -rf ${lib.escapeShellArg config.devenv.state}
    '';
  };
}
