self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  settingsFormat = pkgs.formats.toml { };
in
{
  options.services.academy.backend = {
    enable = lib.mkEnableOption "Bootstrap Academy Backend";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.system}.default;
    };

    chromePackage = lib.mkPackageOption pkgs "ungoogled-chromium" { };

    localDatabase = lib.mkOption {
      type = lib.types.bool;
      default = true;
    };

    localCache = lib.mkOption {
      type = lib.types.bool;
      default = true;
    };

    logLevel = lib.mkOption {
      type = lib.types.str;
      default = "info";
    };

    extraConfigFiles = lib.mkOption {
      type = lib.types.listOf lib.types.path;
      default = [ ];
    };

    settings = lib.mkOption {
      inherit (settingsFormat) type;
      default = { };
    };

    tasks = lib.genAttrs [ "prune-database" "refresh-premium" ] (task: {
      schedule = lib.mkOption {
        type = lib.types.either lib.types.str (lib.types.listOf lib.types.str);
        default = [ ];
      };
    });
  };

  config =
    let
      cfg = config.services.academy.backend;

      settings = settingsFormat.generate "config.toml" cfg.settings;
      ACADEMY_CONFIG = builtins.concatStringsSep ":" (cfg.extraConfigFiles ++ [ settings ]);

      wrapper = pkgs.stdenvNoCC.mkDerivation {
        inherit (cfg.package) pname version;
        src = cfg.package;
        nativeBuildInputs = [ pkgs.makeWrapper ];
        installPhase = ''
          cp -r . $out
          wrapProgram $out/bin/academy --run "[[ \$USER = academy ]] || exec ${pkgs.sudo}/bin/sudo -u academy \"\$0\" \"\$@\"" --set ACADEMY_CONFIG ${lib.escapeShellArg ACADEMY_CONFIG}
        '';
      };
    in
    lib.mkIf cfg.enable {
      assertions = [
        {
          assertion = cfg.localDatabase -> lib.versionAtLeast config.services.postgresql.package.version "17";
          message = "PostgreSQL 17 is required";
        }
      ];

      systemd.services =
        let
          dependencies =
            [ "network-online.target" ]
            ++ (lib.optional cfg.localDatabase "postgresql.service")
            ++ (lib.optional cfg.localCache "redis-academy.service");
          baseConfig = {
            wants = dependencies;
            after = dependencies;

            serviceConfig = {
              User = "academy";
              Group = "academy";
              StateDirectory = "academy";
            };

            environment = {
              inherit ACADEMY_CONFIG;
              RUST_LOG = cfg.logLevel;
            };
          };
        in
        {
          academy-backend = baseConfig // {
            wantedBy = [ "multi-user.target" ];
            script = ''
              ${cfg.package}/bin/academy serve
            '';
          };
        }
        // (lib.mapAttrs' (
          task:
          { schedule }:
          {
            name = "academy-task-${task}";
            value = baseConfig // {
              startAt = schedule;
              script = ''
                ${cfg.package}/bin/academy task ${task}
              '';
            };
          }
        ) cfg.tasks);

      services.postgresql = lib.mkIf cfg.localDatabase {
        enable = true;
        ensureUsers = [
          {
            name = "academy";
            ensureDBOwnership = true;
          }
        ];
        ensureDatabases = [ "academy" ];
      };

      services.redis = lib.mkIf cfg.localCache {
        package = pkgs.valkey;
        servers.academy = {
          enable = true;
          user = "academy";
          save = [ ];
        };
      };

      users.users.academy = {
        isSystemUser = true;
        group = "academy";
      };
      users.groups.academy = { };

      services.academy.backend = {
        settings = {
          database.url = lib.mkIf cfg.localDatabase "host=/run/postgresql user=academy";
          cache.url = lib.mkIf cfg.localCache "redis+unix://${config.services.redis.servers.academy.unixSocket}";
          render.chrome_bin = lib.mkDefault (lib.getExe cfg.chromePackage);
          finance.invoices_archive = lib.mkDefault "/var/lib/academy/invoices";
          finance.credit_notes_archive = lib.mkDefault "/var/lib/academy/credit_notes";
        };

        tasks = {
          prune-database.schedule = lib.mkDefault "hourly";
          refresh-premium.schedule = lib.mkDefault "daily";
        };
      };

      environment.systemPackages = [ wrapper ];
    };
}
