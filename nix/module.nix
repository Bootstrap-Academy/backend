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

    renderDaemon = {
      enable = lib.mkEnableOption "Render Daemon" // {
        default = true;
      };

      package = lib.mkOption {
        type = lib.types.package;
        default = self.packages.${pkgs.system}.render_daemon;
      };

      chromePackage = lib.mkOption {
        type = lib.types.package;
        defaultText = "config.services.academy.backend.renderDaemon.package.passthru.chrome";
      };

      port = lib.mkOption {
        type = lib.types.port;
        default = 8001;
      };

      extraArgs = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [ ];
      };
    };
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
          wrapProgram $out/bin/academy --run "[[ \$USER = academy ]] || exec ${lib.getExe pkgs.sudo} -u academy \"\$0\" \"\$@\"" --set ACADEMY_CONFIG ${lib.escapeShellArg ACADEMY_CONFIG}
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
          httpPort = lib.pipe cfg.settings.http.address [
            (lib.match ".*:([0-9]+)")
            lib.head
            lib.toInt
          ];

          dependencies =
            [ "network-online.target" ]
            ++ (lib.optional cfg.renderDaemon.enable "academy-render-daemon.service")
            ++ (lib.optional cfg.localDatabase "postgresql.service")
            ++ (lib.optional cfg.localCache "redis-academy.service");

          defaultHardening = {
            AmbientCapabilities = "";
            CapabilityBoundingSet = [ "" ];
            DevicePolicy = "closed";
            LockPersonality = true;
            MemoryDenyWriteExecute = true;
            NoNewPrivileges = true;
            PrivateDevices = true;
            PrivateTmp = true;
            PrivateUsers = true;
            ProcSubset = "pid";
            ProtectClock = true;
            ProtectControlGroups = true;
            ProtectHome = true;
            ProtectHostname = true;
            ProtectKernelLogs = true;
            ProtectKernelModules = true;
            ProtectKernelTunables = true;
            ProtectProc = "invisible";
            ProtectSystem = "strict";
            RemoveIPC = true;
            RestrictAddressFamilies = [ "AF_INET AF_INET6 AF_UNIX" ];
            RestrictNamespaces = true;
            RestrictRealtime = true;
            RestrictSUIDSGID = true;
            SocketBindDeny = "any";
            SystemCallArchitectures = "native";
            SystemCallFilter = [
              "@system-service"
              "~@privileged"
              "~@resources"
            ];
            UMask = "0077";
          };

          baseConfig = {
            wants = dependencies;
            after = dependencies;

            serviceConfig = defaultHardening // {
              User = "academy";
              Group = "academy";
              StateDirectory = "academy";
              Restart = "always";
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
              ${lib.getExe cfg.package} serve
            '';
            serviceConfig = baseConfig.serviceConfig // {
              SocketBindAllow = [ "tcp:${toString httpPort}" ];
            };
          };
        }

        // (lib.optionalAttrs cfg.renderDaemon.enable {
          academy-render-daemon = {
            wantedBy = [ "multi-user.target" ];
            wants = [ "network-online.target" ];
            after = [ "network-online.target" ];

            script = ''
              ${lib.getExe cfg.renderDaemon.package} ${
                lib.escapeShellArgs (
                  [
                    "--port=${toString cfg.renderDaemon.port}"
                    "--chrome-bin=${lib.getExe cfg.renderDaemon.chromePackage}"
                  ]
                  ++ cfg.renderDaemon.extraArgs
                )
              }
            '';

            serviceConfig = defaultHardening // {
              DynamicUser = true;
              User = "academy-render-daemon";
              Group = "academy-render-daemon";

              MemoryDenyWriteExecute = false;
              SystemCallFilter = [ ];
              SocketBindAllow = [ "tcp:${toString cfg.renderDaemon.port}" ];
            };

            environment.RUST_LOG = cfg.logLevel;
          };
        })

        // (lib.mapAttrs' (
          task:
          { schedule }:
          {
            name = "academy-task-${task}";
            value = baseConfig // {
              startAt = schedule;
              script = ''
                ${lib.getExe cfg.package} task ${task}
              '';
              serviceConfig = baseConfig.serviceConfig // {
                Type = "oneshot";
                Restart = "on-failure";
              };
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
          course.course_dir = lib.mkDefault self.courses;
          render.daemon_url = lib.mkIf cfg.renderDaemon.enable "http://127.0.0.1:${toString cfg.renderDaemon.port}/";
          finance.invoices_archive = lib.mkDefault "/var/lib/academy/invoices";
          finance.credit_notes_archive = lib.mkDefault "/var/lib/academy/credit_notes";
        };

        renderDaemon = {
          chromePackage = cfg.renderDaemon.package.passthru.chrome;
        };

        tasks = {
          prune-database.schedule = lib.mkDefault "hourly";
          refresh-premium.schedule = lib.mkDefault "daily";
        };
      };

      environment.systemPackages = [ wrapper ];
    };
}
