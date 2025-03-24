{
  lib,
  testers,
  defaultModule,
  interactiveModule,
  ...
}:
testers.runNixOSTest (
  { config, ... }:
  {
    name = "academy-basics";

    nodes.machine = {
      imports = [ defaultModule ];
    };

    interactive.nodes.machine = interactiveModule;

    testScript = ''
      import json

      machine.start()
      machine.succeed("academy --version")

      machine.wait_for_unit("academy-backend.service")
      machine.wait_for_open_port(8000)
      machine.wait_for_unit("academy-render-daemon.service")
      machine.wait_for_open_port(8001)

      machine.wait_for_unit("postfix.service")
      machine.wait_for_open_port(25)

      machine.succeed("curl -s http://127.0.0.1:8000/")

      assert machine.succeed("academy migrate list").strip() == ${
        lib.pipe ../../academy_persistence/postgres/migrations [
          builtins.readDir
          builtins.attrNames
          (map (lib.removeSuffix ".up.sql"))
          (map (lib.removeSuffix ".down.sql"))
          lib.unique
          (lib.sortOn lib.id)
          (map (m: "[applied] ${m}"))
          (builtins.concatStringsSep "\n")
          builtins.toJSON
        ]
      }, "some migrations are missing or have not been applied"

      machine.succeed("academy email test root@localhost")
      machine.wait_until_succeeds("test -e /var/mail/root/new/*", 20)
      machine.succeed("grep 'Email deliverability seems to be working!' /var/mail/root/new/*")

      status = json.loads(machine.succeed("curl -s http://127.0.0.1:8000/health"))
      assert status == {
        "database": True,
        "cache": True,
        "email": True,
      }

      openapi = json.loads(machine.succeed("curl -s http://127.0.0.1:8000/openapi.json"))
      assert openapi["info"]["title"] == "Bootstrap Academy Backend"
      assert openapi["info"]["version"] == "0.0.0-dev"
      assert "https://github.com/Bootstrap-Academy" in openapi["info"]["description"]

      machine.succeed("academy admin user create --admin --verified admin admin@example.com supersecureadminpassword")
      login = json.loads(machine.succeed("curl -s http://127.0.0.1:8000/auth/sessions -X POST -H 'Content-Type: application/json' -d '{\"name_or_email\": \"admin\", \"password\": \"supersecureadminpassword\"}'"))
      assert login["user"]["admin"]
      assert login["user"]["email_verified"]

      assert machine.fail("coredumpctl 2>&1").strip() == "No coredumps found."

      ${lib.pipe config.nodes.machine.systemd.services [
        lib.attrNames
        (lib.filter (name: lib.hasPrefix "academy-" name && !lib.hasPrefix "academy-testing-" name))
        (lib.concatMapStringsSep "\n" (
          name:
          let
            threshold = { academy-render-daemon = 27; }.${name} or 11;
          in
          ''
            machine.log(machine.succeed("SYSTEMD_COLORS=1 systemd-analyze security ${name}.service --threshold=${toString threshold} --no-pager"))
          ''
        ))
      ]}
    '';
  }
)
