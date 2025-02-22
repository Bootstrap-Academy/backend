{ testers, defaultModule, ... }:
testers.runNixOSTest {
  name = "academy-config";

  nodes.default = {
    imports = [ defaultModule ];
  };
  nodes.no_recaptcha = {
    imports = [ defaultModule ];
    services.academy.backend.settings = {
      recaptcha.enable = false;
    };
  };

  testScript = ''
    import json

    start_all()

    default.wait_for_unit("academy-backend.service")
    default.wait_for_open_port(8000)
    assert json.loads(default.succeed("curl -s http://127.0.0.1:8000/auth/recaptcha")) == "test-sitekey"

    no_recaptcha.wait_for_unit("academy-backend.service")
    no_recaptcha.wait_for_open_port(8000)
    assert json.loads(no_recaptcha.succeed("curl -s http://127.0.0.1:8000/auth/recaptcha")) is None
  '';
}
