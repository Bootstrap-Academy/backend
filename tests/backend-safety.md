# Native backend safety regressions

These 13 scenarios exercise the ordinary HTTP router, PostgreSQL transactions and
Valkey-backed sessions using synthetic accounts. They cover hearts/reset/refill,
coin idempotence and ledger rollback, Premium purchase/confirmed renewal/expiry,
public and internal learning policy, refresh/logout/MFA, export and deletion retry.

Build the ordinary backend and run as an unprivileged user with PostgreSQL 18 and
Valkey available. The runner starts its own loopback services, config and disposable
database; it never connects to an existing database or uses provider credentials.

```sh
cargo build --locked --bin academy
nix shell --accept-flake-config --no-write-lock-file --inputs-from . \
  nixpkgs#postgresql_18 nixpkgs#valkey -c bash -euo pipefail -c \
  'python3 -B tests/backend-safety.py --binary target/debug/academy \
    --pg-bin "$(dirname "$(command -v pg_ctl)")" \
    --valkey "$(command -v valkey-server)" --output /tmp/backend-safety-evidence'
```

`--case refresh_logout_and_owner_boundaries` selects a single scenario. A failed
check returns a nonzero exit. `result.json` records each scenario, source and binary
hashes, and cleanup. Backend/service logs are retained in the evidence directory;
all accounts, local processes and database files are removed afterwards.

Registration and purchases use the normal APIs; fixture SQL sets starting balances,
marks synthetic addresses verified and moves paid periods into the past. The renewal
check uses the ordinary opt-in with an SMTP sink that saves no message contents. The
unconfirmed case removes delivery confirmation only inside the owned fixture. The
Daily policy check uses synthetic PDF bytes only to enable an explicit isolated test
cohort; it makes no assertion about document validity or delivery.

The external microservices return synthetic export data and selected failures. These
checks assert the backend's partial-export and durable-deletion contract; existing
per-service suites cover the peers' own behavior. They are not payment-provider or
mail/document acceptance tests. The separate `academy/tests/totp_replay.rs` regression
uses real Valkey to prove exactly one of 32 concurrent second-factor checks succeeds.

CI runs this suite in the existing required `postgres` job after the repository's
marked database tests. No destructive test input or live access token is required.
