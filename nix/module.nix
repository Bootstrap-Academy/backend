self: {
  config,
  lib,
  pkgs,
  ...
}: let
  settingsFormat = pkgs.formats.toml {};
in {
  options.services.academy.backend = {
    enable = lib.mkEnableOption "Bootstrap Academy Backend";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.system}.default;
    };

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
      default = [];
    };

    settings = lib.mkOption {
      inherit (settingsFormat) type;
      default = {};
    };

    tasks = lib.genAttrs ["prune-database"] (task: {
      schedule = lib.mkOption {
        type = lib.types.either lib.types.str (lib.types.listOf lib.types.str);
        default = [];
      };
    });
  };

  config = let
    cfg = config.services.academy.backend;

    settings = settingsFormat.generate "config.toml" cfg.settings;
    ACADEMY_CONFIG = builtins.concatStringsSep ":" (cfg.extraConfigFiles ++ [settings]);

    wrapper = pkgs.stdenvNoCC.mkDerivation {
      inherit (cfg.package) pname version;
      src = cfg.package;
      nativeBuildInputs = [pkgs.makeWrapper];
      installPhase = ''
        cp -r . $out
        wrapProgram $out/bin/academy --run "[[ \$USER = academy ]] || exec ${pkgs.sudo}/bin/sudo -u academy \"\$0\" \"\$@\"" --set ACADEMY_CONFIG ${lib.escapeShellArg ACADEMY_CONFIG}
      '';
    };
  in
    lib.mkIf cfg.enable {
      systemd.services = let
        dependencies = ["network-online.target"] ++ (lib.optional cfg.localDatabase "postgresql.service") ++ (lib.optional cfg.localCache "redis-academy.service");
        baseConfig = {
          wants = dependencies;
          after = dependencies;

          serviceConfig = {
            User = "academy";
            Group = "academy";
          };

          environment = {
            inherit ACADEMY_CONFIG;
            RUST_LOG = cfg.logLevel;
          };
        };
      in
        {
          academy-backend =
            baseConfig
            // {
              wantedBy = ["multi-user.target"];
              script = ''
                ${cfg.package}/bin/academy serve
              '';
            };
        }
        // (
          lib.mapAttrs' (task: {schedule}: {
            name = "academy-task-${task}";
            value =
              baseConfig
              // {
                startAt = schedule;
                script = ''
                  ${cfg.package}/bin/academy task ${task}
                '';
              };
          })
          cfg.tasks
        );

      services.postgresql = lib.mkIf cfg.localDatabase {
        enable = true;
        ensureUsers = [
          {
            name = "academy";
            ensureDBOwnership = true;
          }
        ];
        ensureDatabases = ["academy"];
      };

      services.redis = lib.mkIf cfg.localCache {
        package = pkgs.valkey;
        servers.academy = {
          enable = true;
          user = "academy";
          save = [];
        };
      };

      users.users.academy = {
        isSystemUser = true;
        group = "academy";
      };
      users.groups.academy = {};

      services.academy.backend.settings.database.url = lib.mkIf cfg.localDatabase "host=/run/postgresql user=academy";
      services.academy.backend.settings.cache.url = lib.mkIf cfg.localCache "redis+unix://${config.services.redis.servers.academy.unixSocket}";

      environment.systemPackages = [wrapper];
    };
}
