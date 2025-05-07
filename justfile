_default:
    @just --list

# Reset postgres and valkey
reset: reset-postgres reset-valkey
[private]
alias r := reset

# Reset postgres database
reset-postgres:
    psql <<< 'drop schema public cascade; create schema public;'
[private]
alias rp := reset-postgres

# Reset valkey cache
reset-valkey:
    valkey-cli flushdb
[private]
alias rv := reset-valkey

# Run unit and integration tests
test: test-unit test-extern test-valkey test-email test-postgres
[private]
alias t := test

# Run unit and integration tests with coverage
coverage: coverage-unit coverage-extern coverage-valkey coverage-email coverage-postgres
    lcov -a .lcov-unit.info -a .lcov-extern.info -a .lcov-valkey.info -a .lcov-email.info -a .lcov-postgres.info -o .lcov-combined.info
    genhtml -o .lcov_html .lcov-combined.info
[private]
alias cov := coverage

# Run unit tests
test-unit *args:
    cargo test --locked --all-features --bins --lib {{args}}
    cargo test --locked --all-features --doc {{args}}
[private]
alias tu := test-unit

# Run unit tests with coverage
coverage-unit *args:
    cargo llvm-cov test --lcov --output-path .lcov-test.info --locked --all-features --bins --lib {{args}}
    # cargo llvm-cov test --lcov --output-path .lcov-doc.info --locked --all-features --doc {{args}}
    # lcov -a .lcov-test.info -a .lcov-doc.info -o .lcov-unit.info
    cp .lcov-test.info .lcov-unit.info
    {{ if is_dependency() == "false" { "genhtml -o .lcov_html .lcov-unit.info" } else { "" } }}
[private]
alias covu := coverage-unit

# Run postgres integration tests
test-postgres *args:
    RUST_TEST_THREADS=1 cargo test -p academy_persistence_postgres --locked --all-features --test '*' {{args}}
[private]
alias tp := test-postgres

# Run postgres integration tests with coverage
coverage-postgres *args:
    RUST_TEST_THREADS=1 cargo llvm-cov test --lcov --output-path .lcov-postgres.info -p academy_persistence_postgres --locked --all-features --test '*' {{args}}
    {{ if is_dependency() == "false" { "genhtml -o .lcov_html .lcov-postgres.info" } else { "" } }}
[private]
alias covp := coverage-postgres

# Run valkey integration tests
test-valkey *args:
    RUST_TEST_THREADS=1 cargo test -p academy_cache_valkey --locked --all-features --test '*' {{args}}
[private]
alias tv := test-valkey

# Run valkey integration tests with coverage
coverage-valkey *args:
    RUST_TEST_THREADS=1 cargo llvm-cov test --lcov --output-path .lcov-valkey.info -p academy_cache_valkey --locked --all-features --test '*' {{args}}
    {{ if is_dependency() == "false" { "genhtml -o .lcov_html .lcov-valkey.info" } else { "" } }}
[private]
alias covv := coverage-valkey

# Run email integration tests
test-email *args:
    RUST_TEST_THREADS=1 cargo test -p academy_email_impl --locked --all-features --test '*' {{args}}
[private]
alias tm := test-email

# Run email integration tests with coverage
coverage-email *args:
    RUST_TEST_THREADS=1 cargo llvm-cov test --lcov --output-path .lcov-email.info -p academy_email_impl --locked --all-features --test '*' {{args}}
    {{ if is_dependency() == "false" { "genhtml -o .lcov_html .lcov-email.info" } else { "" } }}
[private]
alias covm := coverage-email

# Run extern integration tests
test-extern *args:
    RUST_TEST_THREADS=1 cargo test -p academy_extern_impl --locked --all-features --test '*' {{args}}
[private]
alias te := test-extern

# Run extern integration tests with coverage
coverage-extern *args:
    RUST_TEST_THREADS=1 cargo llvm-cov test --lcov --output-path .lcov-extern.info -p academy_extern_impl --locked --all-features --test '*' {{args}}
    {{ if is_dependency() == "false" { "genhtml -o .lcov_html .lcov-extern.info" } else { "" } }}
[private]
alias cove := coverage-extern

# Run nix fmt, cargo clippy and cargo test
check: && test
    nix fmt -- --ci
    cargo clippy --all-targets -- -D warnings
[private]
alias c := check

# Create files for a new postgres database migration
new-migration name:
    p="academy_persistence/postgres/migrations/$(date +%Y-%m-%d-%H%M%S)_{{name}}"; mkdir "$p"; touch "$p"/{up,down}.sql
[private]
alias nm := new-migration
