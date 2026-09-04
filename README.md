# Bootstrap Academy Backend
The official backend of [Bootstrap Academy](https://bootstrap.academy/).

If you would like to submit a bug report or feature request, or are looking for general information about the project or the publicly available instances, please refer to the [Bootstrap-Academy repository](https://github.com/Bootstrap-Academy/Bootstrap-Academy).

## Development Setup
1. Install [Nix](https://nixos.org/) with [flakes](https://wiki.nixos.org/wiki/Flakes) enabled:
    - If you are on NixOS, just ensure that flakes are enabled by setting `nix.settings.experimental-features = ["nix-command" "flakes"];` in `configuration.nix`.
    - If you are using a different Linux distribution or OS, follow the instructions on <https://nixos.org/download/> and add the line `experimental-features = nix-command flakes` to `/etc/nix/nix.conf` to enable flakes.
2. Add your user to the [`trusted-users`](https://nix.dev/manual/nix/latest/command-ref/conf-file#conf-trusted-users) Nix option (replace `YOUR_USERNAME` with the actual name of the user account you use for development):
    - If you are on NixOS, set `nix.settings.trusted-users = ["root" "YOUR_USERNAME"];` in `configuration.nix`.
    - If you are using a different Linux distribution or OS, add the line `trusted-users = root YOUR_USERNAME` to `/etc/nix/nix.conf` and run `sudo systemctl restart nix-daemon`.
3. [Install direnv](https://github.com/direnv/direnv/blob/master/docs/installation.md) (optional, but strongly recommended):
    - If you are on NixOS, set `programs.direnv.enable = true;` in `configuration.nix`.
    - If you are using a different Linux distribution or OS, run the command `nix profile add nixpkgs#direnv`. Don't forget to [install the shell hook](https://github.com/direnv/direnv/blob/master/docs/hook.md) (e.g. for bash run `echo 'eval "$(direnv hook bash)"' >> ~/.bashrc`).
4. Clone this repository and `cd` into it.
5. If you installed direnv, run `direnv allow` to automatically load the development environment when entering this repository. Otherwise you need to run `nix develop --no-pure-eval` manually each time to enter the development shell.
6. Run `devenv up` to start a local Postgres database, Valkey cache, SMTP server and some other services needed by the backend.
7. In a different terminal, you can now run `cargo run -- serve`. By default, the Swagger UI documentation is now available on http://127.0.0.1:8000/docs

### Environment Variables
You can set the following additional environment variables to customize the development environment.
If you use direnv, create a file with the name `.env` in the repository root with `NAME=VALUE` pairs on each line.
Otherwise you need to set these variables manually.

- `DEVENV_RUST`: Set to `0` if you already have a Rust toolchain installed which you would like to use instead of the one provided by devenv.
- `RUST_LOG`: Set the tracing filter (log level). See the [`tracing-subscriber` docs](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives) for details.
- `RUST_LOG_PRETTY`: Set to `0` to use the single-line instead of the multi-line log format.

### VSCode Setup
For development in [VSCode](https://code.visualstudio.com/)/[VSCodium](https://vscodium.com/), simply open this repository, install the recommended extensions and confirm restarting the extensions after direnv successfully initialized.

### Useful Commands
- `psql`: Connect to the local Postgres database
- `valkey-cli`: Connect to the local Valkey cache
- `cargo run -- --help`: List all commands provided by the backend CLI
- `cargo run -- check-config -v`: Validate the configuration and print it
- `cargo run -- task prune-database`: Delete sessions that have not been refreshed within `session.refresh_token_ttl`
- `cargo run -- task prune-documents`: Delete invoices and credit notes whose retention period has expired (see [`ARCHITECTURE.md`](ARCHITECTURE.md#scheduled-tasks))
- `cargo run -- task refresh-premium`: Renew the premium memberships whose period has ended (always as a monthly period, see [`ARCHITECTURE.md`](ARCHITECTURE.md#scheduled-tasks))
- `just`: List all recipes provided by the `justfile`
- `generate`: Regenerate `Cargo.nix`. Run this command after changing any `Cargo.toml` or `Cargo.lock` file.
- `generate-clorinde`: Regenerate `academy_persistence/postgres/clorinde`. Run this command after changing the database schema or the queries in `academy_persistence/postgres/queries`.

### Services
- The web interface of [smtp4dev](https://github.com/rnwood/smtp4dev) is available on http://localhost:5000/
- Various services for mocking external APIs are running on ports starting at 8001. See the logs of the `testing-*` services in `devenv up` for details.

## Configuration
The backend reads one or more TOML files listed in the `ACADEMY_CONFIG` environment variable; [`config.toml`](config.toml) holds the defaults and is always loaded.
Every property, its default and whether it has to be set by the deployment are documented in [`docs/config.md`](docs/config.md).

## Tests
This repository contains three different kinds of tests: Unit tests, integration tests and system tests.

Unit tests test only one unit (i.e. function in most cases) at a time and do not rely on any external systems.
To run the unit tests, use the command `just test-unit`.

Integration tests are used to test the integration with other systems such as external APIs or databases.
To run the integration tests, use the corresponding just recipes (e.g. to run the Postgres tests execute `just test-postgres`).

To run all unit and integration tests, use the command `just test`.
It is also possible to generate coverage reports by replacing `test` with `coverage` in any of the previous commands.

System tests are used to test a production build of the backend in an environment that is as realistic as possible (with some exceptions).
For that we use the [NixOS test framework](https://nixos.org/manual/nixos/stable/#sec-nixos-tests) which spawns virtual machines containing a minimal NixOS system running the backend and then runs test scripts written in Python against these VMs.
To run the system tests, use the command `nix build -L .#tests.composite` (if you are on darwin, you may need to set up a [linux builder](https://nixos.org/manual/nixpkgs/stable/#sec-darwin-builder)).
You can also run individual system tests e.g. using `nix build -L .#tests.user`.
