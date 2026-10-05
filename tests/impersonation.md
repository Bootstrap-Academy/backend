# Impersonation provenance and request audit

`impersonation.py` runs against a real backend binary and owned PostgreSQL,
Valkey, SMTP and OAuth loopback fixtures. It is part of the PostgreSQL CI job.
No external provider or real account is involved.

A delegated session may update ordinary profile fields. It cannot create,
replace or remove passwords, change the recovery email, link or unlink OAuth,
initialize/enable/disable MFA, or change profile visibility on either the
owner, old opt-out or support route. These guards use current durable origin
in the write transaction for every target. Normal owner operations and support
withdrawal from an administrator's own sign-in remain available.

Refresh preserves origin. A CLI administrator opening another session passes
on the existing origin, including the unknown operator identity. No descendant
becomes an owner sign-in.

Before the HTTP handler runs, audit middleware captures durable attribution.
It records the final response status from those captured facts, even after
rotation, self-revocation, account deletion or concurrent revocation. Refresh
uses its submitted refresh credential even without an access bearer. A CLI
operator's bearer still attributes invalid JSON, invalid refresh credentials
and oversized-body refusals. A valid ordinary refresh credential takes priority
over an unrelated bearer and cannot be falsely assigned to that operator. A CLI
operator is recorded with `admin_user_id: null`; the target identifies the
account it acts on. No credentials or request bodies enter audit storage.

Migration `2026-10-05-180000_operator_audit` makes only the administrator column
nullable. Schema rollback fails if operator entries exist; application rollback
must keep the nullable column and the audit evidence. Old binaries cannot
read operator entries with their former non-nullable decoder. Bind a compatible
recovery binary before deployment.
