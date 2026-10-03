# Private progress: backend preparation

The new contract is disabled by default (`publication.enabled=false`). Migration
adds private publication fields without changing existing account responses or
legacy opt-out values. Persistent policy activation is a later coordinated release;
once activated it cannot be reverted to the legacy default by config or migration.

Run the real owner/internal APIs against the same isolated PostgreSQL/Valkey
fixture used by `backend-safety.py`:

```sh
set -euo pipefail
cargo build --locked --bin academy
nix shell --accept-flake-config --no-write-lock-file --inputs-from . \
  nixpkgs#postgresql_18 nixpkgs#valkey -c bash -euo pipefail -c \
  'python3 -B tests/publication.py --binary target/debug/academy \
    --pg-bin "$(dirname "$(command -v pg_ctl)")" \
    --valkey "$(command -v valkey-server)" --output /tmp/publication-evidence'
```

Eleven scenarios cover disabled compatibility, internal token audiences, actual
password/OAuth registration, preview/owner/scope boundaries, CAS races, delayed
replays, legacy opt-out, current email verification, receipts in export/erasure,
timed moderation changes, dump/restore, disabled recovery and authority outage,
including loss of the live owner authentication database on all three owner routes.
The password-reset regressions check revoked access/refresh tokens without
withdrawing existing consent, and a publication request queued behind reset on
the account lock. Both real PostgreSQL lock waits are observed before releasing
the blocker; the queued choice must return 401 with the entire publication row
unchanged and the owner absent from the snapshot. Choices, withdrawals and
receipt replays require a current session. Session authority is rechecked inside
the write transaction after taking the same account lock as password reset.
The OAuth fixture seeds the same ephemeral registration cache consumed by the
normal API; no OAuth provider is contacted. Snapshot identities contain exactly
user ID, display name, standard-avatar null and visibility revision; XP and
rankings stay in their owning services. Error and success responses prevent
HTTP storage. No public profile route or UI activation is included in this step.

PostgreSQL repository tests additionally cover old/import/demo defaults,
schema-only rollback/reapply, incomplete receipts and transactional rollback.
The existing migration preservation tests name their intended guard, allowing
later reversible preparation migrations without weakening historical protection.

Evidence contains synthetic fixtures and binary/scenario hashes. The fixture
starts only owned loopback processes, discards mail contents, creates no real
purchase or provider call, and removes its databases/processes afterwards. The
registration/email-change behavior of the existing account API is exercised with
an isolated SMTP sink; publication choices themselves enqueue no mail and do not
alter coins, hearts, premiums or transaction counts.
