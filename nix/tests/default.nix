{
  callPackage,
  lib,
  linkFarm,
  python3,
  self,
  testers,
  writeShellScriptBin,
  writeTextDir,
  system,
}: let
  tests = lib.pipe ./. [
    builtins.readDir
    (lib.filterAttrs (name: type: type == "regular" && isTest name))
    (lib.mapAttrs' (name: _: {
      name = removeSuffix name;
      value = mkTest name;
    }))
  ];

  isTest = name: builtins.any (f: f name) [isPythonTest isNixosTest] && ! builtins.elem name ignored;
  isPythonTest = lib.hasSuffix ".py";
  isNixosTest = lib.hasSuffix ".nix";
  ignored = ["default.nix" "utils.py"];
  removeSuffix = lib.flip lib.pipe [
    (lib.removeSuffix ".py")
    (lib.removeSuffix ".nix")
  ];

  mkTest = name:
    if isPythonTest name
    then mkPythonTest name
    else mkNixosTest name;

  defaultModule = {config, ...}: {
    imports = [self.nixosModules.default];

    services.academy.backend = {
      enable = true;
      package = self.packages.${system}.default.unwrapped;
      logLevel = "info,academy=debug";
      extraConfigFiles = ["/run/academy-backend/secrets.toml"];
      settings = {
        http.address = "127.0.0.1:8000";
        database.acquire_timeout = "2s";
        cache.acquire_timeout = "2s";
        email = {
          smtp_url = "smtp://127.0.0.1:25";
          from = "test@bootstrap.academy";
        };
        internal.shop_url = "http://127.0.0.1:8004/shop/";
        health = {
          database_cache_ttl = "2s";
          cache_cache_ttl = "2s";
          email_cache_ttl = "2s";
        };
        contact.email = "contact@academy";
        recaptcha = {
          enable = lib.mkDefault true;
          siteverify_endpoint_override = "http://127.0.0.1:8001/recaptcha/api/siteverify";
          sitekey = "test-sitekey";
          secret = "test-secret";
          min_score = 0.5;
        };
        vat.validate_endpoint_override = "http://127.0.0.1:8003/validate/";
        paypal = {
          base_url_override = "http://127.0.0.1:8005/";
          client_id = "test-client";
          client_secret = "test-secret";
        };
        oauth2 = {
          enable = true;
          providers = let
            disabled = {
              enable = false;
              client_id = "";
              client_secret = "";
            };
          in {
            github = disabled;
            discord = disabled;
            google = disabled;
            test = {
              name = "Test OAuth2 Provider";
              client_id = "client-id";
              client_secret = "client-secret";
              auth_url = "http://127.0.0.1:8002/oauth2/authorize";
              token_url = "http://127.0.0.1:8002/oauth2/token";
              userinfo_url = "http://127.0.0.1:8002/user";
              userinfo_id_key = "id";
              userinfo_name_key = "name";
              scopes = [];
            };
          };
        };
      };
    };

    systemd.services."academy-testing-recaptcha" = lib.mkIf config.services.academy.backend.settings.recaptcha.enable {
      wantedBy = ["academy-backend.service"];
      before = ["academy-backend.service"];
      script = ''
        ${self.packages.${system}.testing.unwrapped}/bin/academy-testing recaptcha
      '';
    };

    systemd.services."academy-testing-oauth2" = lib.mkIf config.services.academy.backend.settings.oauth2.enable {
      wantedBy = ["academy-backend.service"];
      before = ["academy-backend.service"];
      script = ''
        ${self.packages.${system}.testing.unwrapped}/bin/academy-testing oauth2
      '';
    };

    systemd.services."academy-testing-vat" = {
      wantedBy = ["academy-backend.service"];
      before = ["academy-backend.service"];
      script = ''
        ${self.packages.${system}.testing.unwrapped}/bin/academy-testing vat
      '';
    };

    systemd.services."academy-testing-internal" = {
      wantedBy = ["academy-backend.service"];
      before = ["academy-backend.service"];
      script = ''
        ${self.packages.${system}.testing.unwrapped}/bin/academy-testing internal
      '';
    };

    systemd.services."academy-testing-paypal" = {
      wantedBy = ["academy-backend.service"];
      before = ["academy-backend.service"];
      script = ''
        ${self.packages.${system}.testing.unwrapped}/bin/academy-testing paypal
      '';
    };

    services.postfix = {
      enable = true;
      virtual = "/.*/ root";
      virtualMapType = "pcre";
    };

    systemd.tmpfiles.settings.academy-secrets."/run/academy-backend/secrets.toml".f = {
      user = "academy";
      group = "academy";
      mode = "0400";
      argument = ''
        jwt.secret = "changeme"
      '';
    };
  };

  mkPythonTest = name:
    testers.runNixOSTest {
      name = "academy-${removeSuffix name}";

      nodes.machine = {
        imports = [defaultModule];
        environment.systemPackages = [(python3.withPackages (p: with p; [httpx pyotp]))];
      };

      testScript = ''
        machine.start()
        machine.wait_for_unit("academy-backend.service")
        machine.wait_for_open_port(8000)

        machine.copy_from_host("${./utils.py}", "/root/tests/utils.py")
        machine.copy_from_host("${./${name}}", "/root/tests/${name}")
        machine.succeed("python /root/tests/${name}")
      '';
    };

  mkNixosTest = name: callPackage ./${name} {inherit defaultModule;};

  composite = linkFarm "academy-tests-composite" (builtins.mapAttrs (_: toString) tests);
in
  tests // {inherit composite tests;}
