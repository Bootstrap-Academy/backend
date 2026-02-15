{
  callPackage,
  lib,
  linkFarm,
  self,
  testers,
  writeShellScriptBin,
  writeTextDir,
}:
let
  tests = lib.pipe ./. [
    builtins.readDir
    (lib.filterAttrs (name: type: type == "regular" && isTest name))
    (lib.mapAttrs' (
      name: _: {
        name = removeSuffix name;
        value = mkTest name;
      }
    ))
  ];

  isTest =
    name:
    builtins.any (f: f name) [
      isPythonTest
      isNixosTest
    ]
    && !builtins.elem name ignored;
  isPythonTest = lib.hasSuffix ".py";
  isNixosTest = lib.hasSuffix ".nix";
  ignored = [
    "default.nix"
    "utils.py"
  ];
  removeSuffix = lib.flip lib.pipe [
    (lib.removeSuffix ".py")
    (lib.removeSuffix ".nix")
  ];

  mkTest = name: if isPythonTest name then mkPythonTest name else mkNixosTest name;

  defaultModule =
    { config, pkgs, ... }:
    let
      inherit (pkgs) system;

      testing = lib.getExe self.packages.${system}.testing.unwrapped;
      ports = {
        recaptcha = 8100;
        oauth2 = 8101;
        vat = 8102;
        paypal = 8103;
      };
    in
    {
      imports = [ self.nixosModules.default ];

      services.postgresql.package = pkgs.postgresql_17;

      services.academy.backend = {
        enable = true;
        package = self.packages.${system}.default.unwrapped;
        logLevel = "info,academy=debug";
        extraConfigFiles = [ "/run/academy-backend/secrets.toml" ];
        settings = {
          http.address = "127.0.0.1:8000";
          http.allowed_origins = [ ".*" ];
          database.acquire_timeout = "2s";
          cache.acquire_timeout = "2s";
          email = {
            smtp_url = "smtp://127.0.0.1:25";
            from = "test@bootstrap.academy";
          };
          health = {
            database_cache_ttl = "2s";
            cache_cache_ttl = "2s";
            email_cache_ttl = "2s";
          };
          contact.email = "contact@academy";
          recaptcha = {
            enable = lib.mkDefault true;
            siteverify_endpoint_override = "http://127.0.0.1:${toString ports.recaptcha}/recaptcha/api/siteverify";
            sitekey = "test-sitekey";
            secret = "test-secret";
            min_score = 0.5;
          };
          vat.validate_endpoint_override = "http://127.0.0.1:${toString ports.vat}/validate/";
          paypal = {
            base_url_override = "http://127.0.0.1:${toString ports.paypal}/";
            client_id = "test-client";
            client_secret = "test-secret";
          };
          oauth2 = {
            enable = true;
            providers =
              let
                disabled = {
                  enable = false;
                  client_id = "";
                  client_secret = "";
                };
              in
              {
                github = disabled;
                discord = disabled;
                google = disabled;
                test = {
                  name = "Test OAuth2 Provider";
                  client_id = "client-id";
                  client_secret = "client-secret";
                  auth_url = "http://127.0.0.1:${toString ports.oauth2}/oauth2/authorize";
                  token_url = "http://127.0.0.1:${toString ports.oauth2}/oauth2/token";
                  userinfo_url = "http://127.0.0.1:${toString ports.oauth2}/user";
                  userinfo_id_key = "id";
                  userinfo_name_key = "name";
                  scopes = [ ];
                };
              };
          };
        };
        tasks = {
          prune-database.schedule = [ ];
          refresh-premium.schedule = [ ];
        };
        renderDaemon.package = self.packages.${system}.render_daemon.unwrapped;
      };

      systemd.services."academy-testing-recaptcha" =
        lib.mkIf config.services.academy.backend.settings.recaptcha.enable
          {
            wantedBy = [ "academy-backend.service" ];
            before = [ "academy-backend.service" ];
            script = ''
              ${testing} recaptcha --port ${toString ports.recaptcha}
            '';
          };

      systemd.services."academy-testing-oauth2" =
        lib.mkIf config.services.academy.backend.settings.oauth2.enable
          {
            wantedBy = [ "academy-backend.service" ];
            before = [ "academy-backend.service" ];
            script = ''
              ${testing} oauth2 --port ${toString ports.oauth2}
            '';
          };

      systemd.services."academy-testing-vat" = {
        wantedBy = [ "academy-backend.service" ];
        before = [ "academy-backend.service" ];
        script = ''
          ${testing} vat --port ${toString ports.vat}
        '';
      };

      systemd.services."academy-testing-paypal" = {
        wantedBy = [ "academy-backend.service" ];
        before = [ "academy-backend.service" ];
        script = ''
          ${testing} paypal --port ${toString ports.paypal}
        '';
      };

      services.postfix = {
        enable = true;
        virtual = "/.*/ root";
        virtualMapType = "pcre";
      };

      systemd.tmpfiles.settings.academy-secrets."/run/academy-backend/secrets.toml".C = {
        user = "academy";
        group = "academy";
        mode = "0400";
        argument = builtins.toFile "secrets.toml" ''
          jwt.secret = "changeme"
        '';
      };

      virtualisation.qemu.options = [ "-rtc base=2024-01-01T06:00:00" ];
    };

  interactiveModule =
    { hostPort, ... }:
    {
      _module.args.hostPort = lib.mkDefault 8000;
      services.academy.backend.settings.http.address = lib.mkForce "0.0.0.0:8000";
      networking.firewall.allowedTCPPorts = [ 8000 ];
      virtualisation.graphics = false;
      virtualisation.forwardPorts = [
        {
          from = "host";
          host.port = hostPort;
          guest.port = 8000;
        }
      ];
    };

  mkPythonTest =
    name:
    testers.runNixOSTest {
      name = "academy-${removeSuffix name}";

      nodes.machine =
        { pkgs, ... }:
        {
          imports = [ defaultModule ];
          environment.systemPackages = [
            (pkgs.python3.withPackages (
              p: with p; [
                httpx
                pyotp
                pypdf
              ]
            ))
          ];
        };

      interactive.sshBackdoor.enable = true;
      interactive.defaults = interactiveModule;

      testScript = ''
        machine.start()
        machine.wait_for_unit("academy-backend.service")
        machine.wait_for_open_port(8000)
        machine.wait_for_unit("academy-render-daemon.service")
        machine.wait_for_open_port(8001)

        machine.copy_from_host("${./utils.py}", "/root/tests/utils.py")
        machine.copy_from_host("${./${name}}", "/root/tests/${name}")
        machine.succeed("python /root/tests/${name}")

        assert machine.fail("coredumpctl 2>&1").strip() == "No coredumps found."
      '';
    };

  mkNixosTest = name: callPackage ./${name} { inherit defaultModule interactiveModule; };

  composite = linkFarm "academy-tests-composite" (builtins.mapAttrs (_: toString) tests);
in
tests // { inherit composite; }
