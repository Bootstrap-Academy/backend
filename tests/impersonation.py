"""HTTP/CLI provenance and audit regressions, with owned loopback fixtures only."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from uuid import uuid4

REPO = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("impersonation_sensitive", REPO / "tests/sensitive-writes.py")
sensitive = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = sensitive
spec.loader.exec_module(sensitive)
p = sensitive.publication


def setup(f):
    owner, admin = f.account(), f.admin()
    return owner, admin, p.impersonate(f, admin, owner)


def audit(f, owner, method, path, admin=None):
    actor = "admin_user_id IS NULL" if admin is None else f"admin_user_id='{admin['id']}'"
    return int(
        f.sql(
            f"SELECT count(*) FROM admin_audit_log WHERE {actor} "
            f"AND target_user_id='{owner['id']}' AND method='{method}' AND path='{path}'"
        )
    )


def password_change_cannot_create_owner_credentials(f):
    owner, admin, delegated = setup(f)
    path = "/auth/users/me"
    before = f.state(owner, sessions=True)
    count = audit(f, owner, "PATCH", path, admin)
    f.request(
        path,
        "PATCH",
        {"password": "synthetic replacement password", "display_name": "Must not change"},
        f.token(delegated),
        expected=403,
    )
    assert f.state(owner, sessions=True) == before
    assert audit(f, owner, "PATCH", path, admin) == count + 1
    # The attempted credential cannot be used in a new password sign-in.
    f.request(
        "/auth/sessions",
        "POST",
        {"name_or_email": owner["name"], "password": "synthetic replacement password"},
        expected=401,
    )
    assert (
        f.sql(f"SELECT count(*) FROM user_profiles WHERE last_shared_receipt IS NOT NULL AND user_id='{owner['id']}'")
        == "0"
    )


def admin_password_change_and_revocation_remain_audited(f):
    admin = f.admin()
    path = "/auth/users/me"
    count = audit(f, admin, "PATCH", path, admin)
    f.request(path, "PATCH", {"password": "synthetic administrator replacement"}, f.token(admin))
    assert f.sql(f"SELECT count(*) FROM sessions WHERE user_id='{admin['id']}'") == "0"
    assert audit(f, admin, "PATCH", path, admin) == count + 1


def email_change_cannot_hijack_owner_recovery(f):
    owner, _, delegated = setup(f)
    before = f.state(owner, sessions=True)
    f.request(
        "/auth/users/" + owner["id"],
        "PATCH",
        {"email": "operator-" + uuid4().hex + "@example.com"},
        f.token(delegated),
        expected=403,
    )
    assert f.state(owner, sessions=True) == before


def oauth_link_cannot_create_owner_credentials(f):
    owner, _, delegated = setup(f)
    callback = f.oauth_callback(delegated)
    exchanges = f.oauth.exchanges
    f.request("/auth/oauth/links/me", "POST", callback, f.token(delegated), expected=403)
    assert f.oauth.exchanges == exchanges  # Refused before provider exchange.
    assert f.sql(f"SELECT count(*) FROM oauth2_links WHERE user_id='{owner['id']}'") == "0"
    # With no link, the operator's identity cannot open an owner session.
    flow = f.request("/auth/oauth/authorize", "POST", {"provider_id": "test", "redirect_uri": sensitive.REDIRECT})
    reply = f.request("/auth/sessions/oauth", "POST", {"state": flow["state"], "code": callback["code"]})
    assert reply.get("login") is None


def oauth_unlink_cannot_change_owner_credentials(f):
    owner, admin = f.account(), f.admin()
    link = f.request("/auth/oauth/links/me", "POST", f.oauth_callback(owner), f.token(owner))
    delegated = p.impersonate(f, admin, owner)
    before = f.sql(f"SELECT row_to_json(l)::text FROM oauth2_links l WHERE id='{link['id']}'")
    f.request("/auth/oauth/links/me/" + link["id"], "DELETE", token=f.token(delegated), expected=403)
    assert f.sql(f"SELECT row_to_json(l)::text FROM oauth2_links l WHERE id='{link['id']}'") == before


def mfa_initialization_refused(f):
    owner, _, delegated = setup(f)
    f.request("/auth/users/me/mfa", "POST", token=f.token(delegated), expected=403)
    assert f.sql(f"SELECT count(*) FROM totp_devices WHERE user_id='{owner['id']}'") == "0"


def mfa_enable_refused(f):
    owner, admin = f.account(), f.admin()
    secret = f.request("/auth/users/me/mfa", "POST", token=f.token(owner))
    delegated = p.impersonate(f, admin, owner)
    f.request("/auth/users/me/mfa", "PUT", {"code": sensitive.safety.totp(secret)}, f.token(delegated), expected=403)
    assert f.sql(f"SELECT count(*) FROM totp_devices WHERE user_id='{owner['id']}' AND enabled") == "0"


def mfa_disable_refused(f):
    owner, admin = f.account(), f.admin()
    secret = f.request("/auth/users/me/mfa", "POST", token=f.token(owner))
    f.request("/auth/users/me/mfa", "PUT", {"code": sensitive.safety.totp(secret)}, f.token(owner))
    delegated = p.impersonate(f, admin, owner)
    f.request("/auth/users/me/mfa", "DELETE", token=f.token(delegated), expected=403)
    assert f.sql(f"SELECT count(*) FROM totp_devices WHERE user_id='{owner['id']}' AND enabled") == "1"


def revoked_self_session_remains_audited(f):
    owner, admin, delegated = setup(f)
    path = "/auth/sessions/me/" + delegated["session"]["id"]
    count = audit(f, owner, "DELETE", path, admin)
    f.request(path, "DELETE", token=f.token(delegated))
    assert f.sql(f"SELECT count(*) FROM sessions WHERE id='{delegated['session']['id']}'") == "0"
    assert audit(f, owner, "DELETE", path, admin) == count + 1


def revoked_all_sessions_remain_audited(f):
    owner, admin, delegated = setup(f)
    path = "/auth/sessions/me"
    count = audit(f, owner, "DELETE", path, admin)
    f.request(path, "DELETE", token=f.token(delegated))
    assert f.sql(f"SELECT count(*) FROM sessions WHERE user_id='{owner['id']}'") == "0"
    assert audit(f, owner, "DELETE", path, admin) == count + 1


def refresh_audit(f, with_bearer):
    owner, admin, delegated = setup(f)
    count = audit(f, owner, "PUT", "/auth/session", admin)
    if with_bearer:
        for body, status in [
            ({}, 422),
            ({"refresh_token": "synthetic invalid refresh"}, 401),
            ({"padding": "x" * (2 * 1024 * 1024)}, 413),
        ]:
            f.request("/auth/session", "PUT", body, f.token(delegated), expected=status)
            count += 1
            assert audit(f, owner, "PUT", "/auth/session", admin) == count
            assert f.sql(
                f"SELECT status FROM admin_audit_log WHERE admin_user_id='{admin['id']}' "
                f"AND target_user_id='{owner['id']}' AND path='/auth/session' ORDER BY at DESC LIMIT 1"
            ) == str(status)
    renewed = f.request(
        "/auth/session",
        "PUT",
        {"refresh_token": delegated["refresh_token"]},
        f.token(delegated) if with_bearer else None,
    )
    assert renewed["session"]["id"] == delegated["session"]["id"]
    assert audit(f, owner, "PUT", "/auth/session", admin) == count + 1
    assert (
        f.sql(f"SELECT origin||'|'||impersonated_by FROM sessions WHERE id='{renewed['session']['id']}'")
        == "impersonation|" + admin["id"]
    )
    f.request("/auth/session", token=f.token(delegated), expected=401)
    _, choice = f.preview_choice(renewed)
    assert f.publication(method="PUT", body=choice, token=f.token(renewed), expected=403) == p.REFUSED


def refreshed_bearer_remains_audited(f):
    refresh_audit(f, True)


def refresh_without_bearer_remains_audited(f):
    refresh_audit(f, False)


def refresh_uses_actual_credential_instead_of_unrelated_bearer(f):
    owner, admin, delegated = setup(f)
    other = f.account()
    count = audit(f, owner, "PUT", "/auth/session", admin)
    renewed_other = f.request("/auth/session", "PUT", {"refresh_token": other["refresh_token"]}, f.token(delegated))
    assert audit(f, owner, "PUT", "/auth/session", admin) == count
    assert f.sql(f"SELECT origin FROM sessions WHERE id='{renewed_other['session']['id']}'") == "sign_in"
    renewed = f.request("/auth/session", "PUT", {"refresh_token": delegated["refresh_token"]}, f.token(renewed_other))
    assert audit(f, owner, "PUT", "/auth/session", admin) == count + 1
    assert f.sql(f"SELECT origin FROM sessions WHERE id='{renewed['session']['id']}'") == "impersonation"


def account_deletion_remains_audited(f):
    owner, admin, delegated = setup(f)
    path = "/auth/users/me"
    count = audit(f, owner, "DELETE", path, admin)
    f.request(path, "DELETE", token=f.token(delegated))
    assert f.sql(f"SELECT count(*) FROM users WHERE id='{owner['id']}'") == "0"
    assert audit(f, owner, "DELETE", path, admin) == count + 1


def cli_support_and_foreign_legacy_withdrawals_refused(f):
    admin, owner = f.admin(), f.account()
    for account in (admin, owner):
        _, choice = f.preview_choice(account)
        f.publication(method="PUT", body=choice, token=f.token(account))
    bearer = f.command(f.args.binary, "admin", "session", "impersonate", admin["name"])
    for account in (admin, owner):
        before = f.sql(p.PROFILE_ROW.format(account["id"]))
        revision = f.publication(token=f.token(account))["visibility_revision"]
        f.publication(
            "/auth/admin/users/" + account["id"] + "/publication/withdraw",
            method="POST",
            body={"expected_revision": revision, "request_id": str(uuid4())},
            token=bearer,
            expected=403,
        )
        f.request("/auth/users/" + account["id"], "PATCH", {"leaderboard_opt_out": True}, bearer, expected=403)
        assert f.sql(p.PROFILE_ROW.format(account["id"])) == before
    # A normal MFA-confirmed admin retains the support withdrawal.
    revision = f.publication(token=f.token(owner))["visibility_revision"]
    reply = f.publication(
        "/auth/admin/users/" + owner["id"] + "/publication/withdraw",
        method="POST",
        body={"expected_revision": revision, "request_id": str(uuid4())},
        token=f.token(admin),
    )
    assert reply["receipt"]["source"] == "support"


def cli_requests_and_descendants_remain_audited(f):
    admin, owner = f.admin(), f.account()
    bearer = f.command(f.args.binary, "admin", "session", "impersonate", admin["name"])
    path = "/auth/users/" + owner["id"]
    count = audit(f, owner, "PATCH", path)
    f.request(path, "PATCH", {"display_name": "Synthetic CLI update"}, bearer)
    assert audit(f, owner, "PATCH", path) == count + 1
    descendant = f.request("/auth/sessions/" + owner["id"], "POST", token=bearer)
    assert (
        f.sql(
            f"SELECT origin||'|'||(impersonated_by IS NULL)::text FROM sessions WHERE id='{descendant['session']['id']}'"
        )
        == "impersonation|true"
    )
    count = audit(f, owner, "PUT", "/auth/session")
    renewed = f.request("/auth/session", "PUT", {"refresh_token": descendant["refresh_token"]})
    assert audit(f, owner, "PUT", "/auth/session") == count + 1
    _, choice = f.preview_choice(renewed)
    f.publication(method="PUT", body=choice, token=f.token(renewed), expected=403)
    assert audit(f, owner, "PUT", p.OWNER) == 1


def concurrent_revocation_keeps_captured_actor(f):
    target, actor = sorted((f.admin(), f.admin()), key=lambda user: user["id"])
    bearer = f.command(f.args.binary, "admin", "session", "impersonate", actor["name"])
    session = f.request("/auth/session", token=bearer)
    path = "/auth/users/" + target["id"]
    count = audit(f, target, "PATCH", path)
    blocker = sensitive.OwnerLock(f, target)
    try:
        with ThreadPoolExecutor(max_workers=1) as pool:
            pending = pool.submit(f.request, path, "PATCH", {"display_name": "Must not change"}, bearer, None)
            assert sensitive.wait_blocked(f, pending)
            f.request("/auth/sessions/me/" + session["id"], "DELETE", token=f.token(actor))
            blocker.release()
            assert pending.result(timeout=25)[0] == 401
        assert audit(f, target, "PATCH", path) == count + 1
    finally:
        blocker.close()


CASES = [
    password_change_cannot_create_owner_credentials,
    admin_password_change_and_revocation_remain_audited,
    email_change_cannot_hijack_owner_recovery,
    oauth_link_cannot_create_owner_credentials,
    oauth_unlink_cannot_change_owner_credentials,
    mfa_initialization_refused,
    mfa_enable_refused,
    mfa_disable_refused,
    revoked_self_session_remains_audited,
    revoked_all_sessions_remain_audited,
    refreshed_bearer_remains_audited,
    refresh_without_bearer_remains_audited,
    refresh_uses_actual_credential_instead_of_unrelated_bearer,
    account_deletion_remains_audited,
    cli_support_and_foreign_legacy_withdrawals_refused,
    cli_requests_and_descendants_remain_audited,
    concurrent_revocation_keeps_captured_actor,
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--pg-bin", type=Path, required=True)
    parser.add_argument("--valkey", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error("run as an unprivileged user")
    for name in ("binary", "pg_bin", "valkey"):
        setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=False)
    result = {
        "passed": False,
        "cases": [],
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip(),
        "scenario_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "binary_sha256": p.safety.binary_digest(args.binary),
    }
    try:
        with sensitive.Fixture(args) as f:
            f.activate()
            for case in CASES:
                started = time.monotonic()
                try:
                    case(f)
                    row = {"name": case.__name__, "passed": True}
                except Exception as error:
                    row = {"name": case.__name__, "passed": False, "error": str(error)}
                row["seconds"] = round(time.monotonic() - started, 3)
                result["cases"].append(row)
                (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
                print(case.__name__ + ": " + str(row["passed"]), flush=True)
        result["passed"] = all(case["passed"] for case in result["cases"])
    finally:
        result["fixture_removed"] = not f.base.exists() if "f" in locals() else None
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
