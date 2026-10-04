"""Private-progress HTTP regressions using the existing owned PG/Valkey fixture."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import os
import socket
from pathlib import Path
import subprocess
import time
import urllib.error
import urllib.request
from uuid import uuid4

REPO = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("backend_safety", REPO / "tests/backend-safety.py")
safety = importlib.util.module_from_spec(spec)
spec.loader.exec_module(safety)
ROOT = "/auth/_internal/profile-publications/"
OWNER = "/auth/users/me/publication"


class Fixture(safety.Fixture):
    publication_enabled = False
    database = "safety"

    def sql(self, statement):
        return self.command(
            self.args.pg_bin / "psql",
            "-X",
            "-qAt",
            "-v",
            "ON_ERROR_STOP=1",
            "-h",
            "127.0.0.1",
            "-p",
            self.ports["pg"],
            "-U",
            "safety",
            "-d",
            self.database,
            "-c",
            statement,
        )

    def write_config(self):
        super().write_config()
        path = self.base / "fixture.toml"
        config = path.read_text().replace('/safety"', f'/{self.database}"')
        # Keep deliberate PG outages inside the HTTP deadline regardless of
        # when the pool's background connection cleanup runs.
        config = config.replace("[database]\n", '[database]\nacquire_timeout = "2s"\n', 1)
        path.write_text(config + f"\n[publication]\nenabled = {str(self.publication_enabled).lower()}\n")

    def publication(self, path=OWNER, *, method="GET", body=None, token=None, expected=200):
        headers = {"Content-Type": "application/json"}
        if token:
            headers["Authorization"] = "Bearer " + token
        request = urllib.request.Request(
            f"http://127.0.0.1:{self.ports['api']}{path}",
            method=method,
            headers=headers,
            data=None if body is None else json.dumps(body).encode(),
        )
        try:
            response = urllib.request.urlopen(request, timeout=20)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            assert response.headers["Cache-Control"] == "private, no-store"
            assert "Authorization" in ",".join(response.headers.get_all("Vary", []))
            assert response.status == expected, f"{method} {path}: {response.status}, expected {expected}"
            data = response.read()
            try:
                return json.loads(data)
            except json.JSONDecodeError:
                return data.decode()

    def activate(self):
        self.sql("UPDATE profile_publication_state SET policy_active=true")
        self.publication_enabled = True
        self.restart()

    def preview_choice(self, account):
        preview = self.publication(OWNER + "-preview", token=self.token(account))
        body = {
            "profile_visibility": "shared",
            "expected_revision": preview["publication"]["visibility_revision"],
            "request_id": str(uuid4()),
            "scope_version": preview["scope_version"],
            "notice_hash": preview["notice_hash"],
            "preview_token": preview["preview_token"],
        }
        assert hashlib.sha256(preview["notice"].encode()).hexdigest() == body["notice_hash"]
        return preview, body

    def counters(self):
        return (
            self.smtp.messages,
            self.sql(
                "SELECT (SELECT count(*) FROM transactions),(SELECT count(*) FROM internal_heart_operations),(SELECT count(*) FROM premium),(SELECT coalesce(sum(coins),0) FROM coins),(SELECT coalesce(sum(hearts),0) FROM hearts)"
            ),
        )


def disabled_contract_and_service_auth(f):
    user = f.account()
    token = f.token(user)
    legacy = f.request("/auth/users/me", token=token)
    assert legacy["leaderboard_opt_out"] is False
    assert not any(key.startswith(("publication", "visibility", "shared_")) for key in legacy)
    assert f.sql(f"SELECT profile_visibility FROM user_profiles WHERE user_id='{user['id']}'") == "private"
    f.publication(token=token, expected=503)
    f.publication(OWNER + "-preview", token=token, expected=503)
    f.publication(expected=401)
    for path in (ROOT + "epoch", ROOT + "snapshot"):
        f.publication(path, expected=401)
        f.publication(path, token=token, expected=401)
        f.publication(path, token=f.shop, expected=401)
        authority = f.publication(path, token=f.auth)
        assert authority["policy_active"] is False and authority["publishing_enabled"] is False
        if "participants" in authority:
            assert authority["participants"] == []
    # OAuth registrations still go through the normal account API and private DB default.
    registration = {
        "provider_id": "github",
        "remote_user": {"id": "publication-" + uuid4().hex, "name": "Synthetic OAuth owner"},
    }
    oauth_token = uuid4().hex + uuid4().hex

    # rmp-serde's compact struct encoding, as used by the real cache service.
    def pack_string(value):
        value = value.encode()
        return (bytes([0xA0 + len(value)]) if len(value) < 32 else b"\xd9" + bytes([len(value)])) + value

    payload = (
        b"\x92"
        + pack_string(registration["provider_id"])
        + b"\x92"
        + pack_string(registration["remote_user"]["id"])
        + pack_string(registration["remote_user"]["name"])
    )
    parts = [b"SET", ("oauth2_registration:" + oauth_token).encode(), payload, b"EX", b"600"]
    wire = b"*5\r\n" + b"".join(b"$" + str(len(part)).encode() + b"\r\n" + part + b"\r\n" for part in parts)
    with socket.create_connection(("127.0.0.1", f.ports["cache"])) as cache:
        cache.sendall(wire)
        assert cache.recv(1024) == b"+OK\r\n"
    created = f.request(
        "/auth/users",
        "POST",
        {
            "name": "oauth" + uuid4().hex[:12],
            "display_name": "OAuth fixture",
            "email": "oauth-" + uuid4().hex + "@example.com",
            "oauth_register_token": oauth_token,
            "terms_version": "safety-test-terms",
            "age_confirmed": True,
        },
        expected=None,
    )
    # The synthetic cache entry exercises ordinary registration without a provider call.
    assert created[0] == 200, f"synthetic OAuth registration returned {created[0]}"
    assert (
        f.sql(f"SELECT profile_visibility FROM user_profiles WHERE user_id='{created[1]['user']['id']}'") == "private"
    )


def owner_preview_and_exact_projection(f):
    a, b = f.account(), f.account()
    f.activate()
    baseline = f.counters()
    state = f.sql("SELECT row_to_json(s)::text FROM profile_publication_state s")
    preview, body = f.preview_choice(a)
    assert f.sql("SELECT row_to_json(s)::text FROM profile_publication_state s") == state
    assert preview["publication"]["profile_visibility"] == "private"
    assert set(preview["profile"]) == {"user_id", "display_name", "avatar_url", "visibility_revision"}
    assert preview["profile"]["avatar_url"] is None
    f.publication(method="PUT", body=body, token=f.token(b), expected=422)
    f.publication(
        method="PUT", body={**body, "profile_visibility": "private", "admin": True}, token=f.token(a), expected=422
    )
    result = f.publication(method="PUT", body=body, token=f.token(a))
    assert result["current"]["profile_visibility"] == "shared" and not result["replayed"]
    snapshot = f.publication(ROOT + "snapshot", token=f.auth)
    expected_identity = {**preview["profile"], "visibility_revision": result["current"]["visibility_revision"]}
    assert snapshot["participants"] == [expected_identity]
    assert f.publication(ROOT + "epoch", token=f.auth)["publication_epoch"] == snapshot["publication_epoch"]
    f.request("/auth/users/" + a["id"], token=f.token(b), expected=403)
    assert f.counters() == baseline
    return a, body


def race_replay_and_legacy_clients(f):
    a = f.account()
    preview, body = f.preview_choice(a)

    def choose(_):
        status, result = f.request(OWNER, "PUT", {**body, "request_id": str(uuid4())}, token=f.token(a), expected=None)
        return status, result

    with ThreadPoolExecutor(max_workers=8) as executor:
        results = list(executor.map(choose, range(8)))
    assert sorted(status for status, _ in results) == [200] + [409] * 7
    winner = next(result for status, result in results if status == 200)
    original = {**body, "request_id": winner["receipt"]["request_id"]}
    before = f.publication(ROOT + "epoch", token=f.auth)
    f.request("/auth/users/me", "PATCH", {"leaderboard_opt_out": True}, token=f.token(a))
    withdrawn = f.publication(token=f.token(a))
    assert withdrawn["profile_visibility"] == "private"
    retry = f.publication(method="PUT", body=original, token=f.token(a))
    assert retry["replayed"] and retry["receipt"] == winner["receipt"]
    assert retry["current"]["profile_visibility"] == "private"
    assert f.publication(ROOT + "epoch", token=f.auth)["publication_epoch"] != before["publication_epoch"]
    f.request(
        "/auth/users/me",
        "PATCH",
        {"leaderboard_opt_out": False, "display_name": "Renamed private fixture"},
        token=f.token(a),
    )
    assert f.request("/auth/users/me", token=f.token(a))["leaderboard_opt_out"] is True
    f.publication(
        method="PUT",
        body={**body, "expected_revision": withdrawn["visibility_revision"], "request_id": str(uuid4())},
        token=f.token(a),
        expected=422,
    )
    return a


RESET_CODE = "ABCD-EFGH-IJKL-MNOP"
RESET_PASSWORD = "synthetic publication reset password"
ACCOUNT_LOCK = "SELECT id FROM users WHERE id=$1 FOR UPDATE"


def seed_password_reset(f, owner):
    # Seed the real ephemeral rmp-serde cache entry without sending reset mail.
    def pack_string(value):
        data = value.encode()
        return (bytes([0xA0 + len(data)]) if len(data) < 32 else b"\xd9" + bytes([len(data)])) + data

    payload = b"\x92" + pack_string(owner["name"] + "@example.com") + pack_string(RESET_CODE)
    parts = [b"SET", ("reset_password_code:v2:" + owner["id"]).encode(), payload, b"EX", b"60"]
    wire = b"*5\r\n" + b"".join(b"$" + str(len(part)).encode() + b"\r\n" + part + b"\r\n" for part in parts)
    with socket.create_connection(("127.0.0.1", f.ports["cache"])) as cache:
        cache.sendall(wire)
        assert cache.recv(1024) == b"+OK\r\n"


def reset_password(f, owner):
    return f.request(
        "/auth/password_reset",
        "PUT",
        {"email": owner["name"] + "@example.com", "code": RESET_CODE, "password": RESET_PASSWORD},
        expected=None,
    )


def wait_for_account_locks(f, count):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        waiting = int(
            f.sql(
                "SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() "
                "AND wait_event_type='Lock' AND query LIKE '%" + ACCOUNT_LOCK + "%'"
            )
        )
        if waiting >= count:
            return waiting
        time.sleep(0.02)
    raise RuntimeError("controlled HTTP requests did not reach their account locks")


def reset_preserves_consent_and_revokes_publication_authority(f):
    owner = f.account()
    _, choice = f.preview_choice(owner)
    shared = f.publication(method="PUT", body=choice, token=f.token(owner))
    seed_password_reset(f, owner)
    status, _ = reset_password(f, owner)
    assert status == 200
    f.publication(token=f.token(owner), expected=401)
    f.publication(OWNER + "-preview", token=f.token(owner), expected=401)
    f.publication(method="PUT", body=choice, token=f.token(owner), expected=401)
    f.publication(
        method="PUT",
        body={
            "profile_visibility": "private",
            "expected_revision": shared["current"]["visibility_revision"],
            "request_id": str(uuid4()),
        },
        token=f.token(owner),
        expected=401,
    )
    f.request("/auth/session", "PUT", {"refresh_token": owner["refresh_token"]}, expected=401)
    assert f.sql(f"SELECT count(*) FROM sessions WHERE user_id='{owner['id']}'") == "0"
    owner.update(f.login(owner["name"], RESET_PASSWORD))
    assert f.publication(token=f.token(owner)) == shared["current"]
    assert owner["id"] in [p["user_id"] for p in f.publication(ROOT + "snapshot", token=f.auth)["participants"]]


def queued_publication_cannot_gain_authority_after_reset(f):
    owner = f.account()
    _, choice = f.preview_choice(owner)
    seed_password_reset(f, owner)
    profile_before = f.sql(f"SELECT row_to_json(p)::text FROM user_profiles p WHERE user_id='{owner['id']}'")
    blocker = subprocess.Popen(
        [
            str(f.args.pg_bin / "psql"),
            "-X",
            "-qAt",
            "-v",
            "ON_ERROR_STOP=1",
            "-h",
            "127.0.0.1",
            "-p",
            str(f.ports["pg"]),
            "-U",
            "safety",
            "-d",
            f.database,
        ],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        env=f.env,
    )
    try:
        blocker.stdin.write(f"BEGIN;\nSELECT id FROM users WHERE id='{owner['id']}' FOR UPDATE;\nSELECT 'locked';\n")
        blocker.stdin.flush()
        assert blocker.stdout.readline().strip() == owner["id"]
        assert blocker.stdout.readline().strip() == "locked"
        with ThreadPoolExecutor(max_workers=2) as pool:
            recovery = pool.submit(reset_password, f, owner)
            wait_for_account_locks(f, 1)
            consent = pool.submit(f.request, OWNER, "PUT", choice, f.token(owner), None)
            # Both requests must actually be waiting; reset reached the lock first.
            wait_for_account_locks(f, 2)
            blocker.stdin.write("COMMIT;\n")
            blocker.stdin.flush()
            reset_status, _ = recovery.result(timeout=15)
            choice_status, _ = consent.result(timeout=15)
        assert reset_status == 200 and choice_status == 401
        f.request("/auth/session", token=f.token(owner), expected=401)
        assert f.sql(f"SELECT count(*) FROM sessions WHERE user_id='{owner['id']}'") == "0"
        assert (
            f.sql(f"SELECT row_to_json(p)::text FROM user_profiles p WHERE user_id='{owner['id']}'") == profile_before
        )
        assert f.sql(f"SELECT profile_visibility FROM user_profiles WHERE user_id='{owner['id']}'") == "private"
        assert owner["id"] not in [p["user_id"] for p in f.publication(ROOT + "snapshot", token=f.auth)["participants"]]
    finally:
        if blocker.poll() is None:
            blocker.stdin.write("ROLLBACK;\n\\q\n")
            blocker.stdin.flush()
            blocker.communicate(timeout=10)


def current_verification_withdrawal_and_export(f):
    a = f.account()
    _, body = f.preview_choice(a)
    baseline = f.counters()
    f.sql(f"UPDATE users SET email_verified=false WHERE id='{a['id']}'")
    f.publication(method="PUT", body=body, token=f.token(a), expected=403)
    f.sql(f"UPDATE users SET email_verified=true WHERE id='{a['id']}'")
    shared = f.publication(method="PUT", body=body, token=f.token(a))
    f.request("/auth/users/me", "PATCH", {"email": "changed-" + uuid4().hex + "@example.com"}, token=f.token(a))
    # Email change invalidates tokens; sign in again with the same owner.
    a.update(f.login(a["name"], a["password"]))
    snapshot = f.publication(ROOT + "snapshot", token=f.auth)
    assert a["id"] not in [item["user_id"] for item in snapshot["participants"]]
    request = {
        "profile_visibility": "private",
        "expected_revision": shared["current"]["visibility_revision"],
        "request_id": str(uuid4()),
    }
    withdrawn = f.publication(method="PUT", body=request, token=f.token(a))
    assert f.publication(method="PUT", body=request, token=f.token(a))["receipt"] == withdrawn["receipt"]
    assert f.counters() == baseline
    export = f.request("/auth/users/me/export", token=f.token(a))
    assert export["account"]["publication"]["last_private_receipt"] == withdrawn["receipt"]
    assert export["account"]["publication"]["last_shared_receipt"] == shared["receipt"]
    assert f.publication(ROOT + "snapshot", token=f.auth)["scope_version"] == "academy-verified-v1"


def moderation_boundaries_invalidate_cached_membership(f):
    a = f.account()
    _, body = f.preview_choice(a)
    f.publication(method="PUT", body=body, token=f.token(a))
    case_id = str(uuid4())
    f.sql(
        f"INSERT INTO moderation_targets(kind,id,subject) VALUES('account','{a['id']}','{a['id']}');"
        f"INSERT INTO moderation_cases(id,target_kind,target_id,subject,source,private_evidence) VALUES('{case_id}','account','{a['id']}','{a['id']}','own_review','{{}}');"
        f"INSERT INTO moderation_holds(case_id,target_kind,target_id,effect,starts_at,ends_at) VALUES('{case_id}','account','{a['id']}','restrict',clock_timestamp()+interval '2 seconds',clock_timestamp()+interval '4 seconds')"
    )
    cached = f.publication(ROOT + "snapshot", token=f.auth)
    assert a["id"] in [entry["user_id"] for entry in cached["participants"]]
    time.sleep(2.2)
    blocked_epoch = f.publication(ROOT + "epoch", token=f.auth)
    assert blocked_epoch["publication_epoch"] != cached["publication_epoch"]
    blocked = f.publication(ROOT + "snapshot", token=f.auth)
    assert a["id"] not in [entry["user_id"] for entry in blocked["participants"]]
    time.sleep(2.1)
    restored_epoch = f.publication(ROOT + "epoch", token=f.auth)
    assert restored_epoch["publication_epoch"] != blocked_epoch["publication_epoch"]
    restored = f.publication(ROOT + "snapshot", token=f.auth)
    assert a["id"] in [entry["user_id"] for entry in restored["participants"]]


def erasure_removes_receipts_and_membership(f):
    a = f.account()
    _, body = f.preview_choice(a)
    f.publication(method="PUT", body=body, token=f.token(a))
    before = f.publication(ROOT + "epoch", token=f.auth)
    f.request("/auth/users/me", "DELETE", token=f.token(a))
    assert f.sql(f"SELECT count(*) FROM user_profiles WHERE user_id='{a['id']}'") == "0"
    snapshot = f.publication(ROOT + "snapshot", token=f.auth)
    assert snapshot["publication_epoch"] != before["publication_epoch"]
    assert a["id"] not in [entry["user_id"] for entry in snapshot["participants"]]
    f.publication(token=f.token(a), expected=401)


def scope_updates_invalidate_old_authority(f):
    before = f.publication(ROOT + "epoch", token=f.auth)
    f.sql("SELECT profile_publication_recheck('future-scope','future-notice')")
    # A backend code/notice change cannot reuse a positive snapshot from an old scope.
    current = f.publication(ROOT + "epoch", token=f.auth)
    assert current["publication_epoch"] != before["publication_epoch"]
    assert current["epoch_revision"] > before["epoch_revision"]
    assert current["scope_version"] == "academy-verified-v1"


def dump_restore_and_disabled_recovery(f):
    before = f.publication(ROOT + "epoch", token=f.auth)
    dump = f.base / "publication.dump"
    f.command(
        f.args.pg_bin / "pg_dump",
        "-h",
        "127.0.0.1",
        "-p",
        f.ports["pg"],
        "-U",
        "safety",
        "-d",
        "safety",
        "-Fc",
        "-f",
        dump,
    )
    f.sql("CREATE DATABASE publication_restore")
    f.command(
        f.args.pg_bin / "pg_restore",
        "-h",
        "127.0.0.1",
        "-p",
        f.ports["pg"],
        "-U",
        "safety",
        "-d",
        "publication_restore",
        dump,
    )
    f.database = "publication_restore"
    f.publication_enabled = False
    f.restart()
    f.command(f.args.binary, "migrate", "up", label="restored-migrations")
    restored = f.publication(ROOT + "epoch", token=f.auth)
    assert restored["policy_active"] and not restored["publishing_enabled"]
    assert restored["publication_epoch"] == before["publication_epoch"]
    assert f.publication(ROOT + "snapshot", token=f.auth)["participants"] == []
    f.publication(token="invalid", expected=401)
    f.publication_enabled = True
    f.restart()
    assert f.publication(ROOT + "epoch", token=f.auth)["publication_epoch"] == before["publication_epoch"]


def authority_outage_is_closed(f):
    owner = f.account()
    owner_token = f.token(owner)
    f.publication(ROOT + "snapshot", token=f.auth)
    f.command(f.args.pg_bin / "pg_ctl", "-D", f.base / "pg", "-m", "fast", "-w", "stop", label="publication-outage")
    f.pg_started = False
    assert f.publication(ROOT + "snapshot", token=f.auth, expected=503) == {"detail": "publication_unavailable"}
    for path, method, body in (
        (OWNER, "GET", None),
        (OWNER + "-preview", "GET", None),
        (OWNER, "PUT", {"profile_visibility": "private", "expected_revision": 0, "request_id": str(uuid4())}),
    ):
        assert f.publication(path, method=method, body=body, token=owner_token, expected=503) == {
            "detail": "publication_unavailable"
        }


def support_admin(f):
    admin = f.account(admin=True)
    secret = f.request("/auth/users/me/mfa", "POST", token=f.token(admin))
    f.request("/auth/users/me/mfa", "PUT", {"code": safety.totp(secret)}, f.token(admin))
    admin.update(f.login(admin["name"], admin["password"], mfa_code=safety.totp(secret, offset=1)))
    assert admin["session"]["mfa_verified"]
    return admin


def support_permissions_withdrawal_and_replay(f):
    owner, ordinary, unverified_admin = f.account(), f.account(), f.account(admin=True)
    admin = support_admin(f)
    path = f"/auth/admin/users/{owner['id']}/publication"
    command = {"expected_revision": 0, "request_id": str(uuid4())}
    for token, expected in [(None, 401), (f.token(ordinary), 403), (f.token(unverified_admin), 403)]:
        f.publication(path, token=token, expected=expected)
        f.publication(path + "/withdraw", method="POST", body=command, token=token, expected=expected)
    f.publication(path, token=f.auth, expected=401)
    f.publication(path, token=f.token(admin))
    baseline = f.counters()
    preview, share_body = f.preview_choice(owner)
    first = f.publication(method="PUT", body=share_body, token=f.token(owner))
    command["expected_revision"] = first["current"]["visibility_revision"]
    before = f.publication(ROOT + "epoch", token=f.auth)
    for extra in [{"profile_visibility": "shared"}, {"source": "owner"}, {"preview_token": preview["preview_token"]}]:
        f.publication(path + "/withdraw", method="POST", body={**command, **extra}, token=f.token(admin), expected=422)
    f.publication(path, method="PUT", body=share_body, token=f.token(admin), expected=405)
    f.publication(
        path + "/withdraw", method="POST", body={**command, "expected_revision": 99}, token=f.token(admin), expected=409
    )
    withdrawn = f.publication(path + "/withdraw", method="POST", body=command, token=f.token(admin))
    assert withdrawn["current"]["profile_visibility"] == "private"
    assert withdrawn["receipt"]["source"] == "support"
    assert withdrawn["receipt"]["profile_visibility"] == "private"
    assert withdrawn["receipt"]["scope_version"] is None
    assert f.publication(ROOT + "epoch", token=f.auth)["publication_epoch"] != before["publication_epoch"]
    assert owner["id"] not in [p["user_id"] for p in f.publication(ROOT + "snapshot", token=f.auth)["participants"]]
    replay = f.publication(path + "/withdraw", method="POST", body=command, token=f.token(admin))
    assert replay["replayed"] and replay["receipt"] == withdrawn["receipt"]
    assert (
        f.publication(method="PUT", body=share_body, token=f.token(owner))["current"]["profile_visibility"] == "private"
    )
    _, new_share = f.preview_choice(owner)
    reshared = f.publication(method="PUT", body=new_share, token=f.token(owner))
    delayed = f.publication(path + "/withdraw", method="POST", body=command, token=f.token(admin))
    assert delayed["replayed"] and delayed["current"] == reshared["current"]
    assert delayed["receipt"] == withdrawn["receipt"]
    # Deactivation and an unverified email do not remove support's ability to withdraw.
    case_id = str(uuid4())
    f.sql(
        f"UPDATE users SET email_verified=false WHERE id='{owner['id']}';"
        f"INSERT INTO moderation_targets(kind,id,subject) VALUES('account','{owner['id']}','{owner['id']}');"
        f"INSERT INTO moderation_cases(id,target_kind,target_id,subject,source,private_evidence) VALUES('{case_id}','account','{owner['id']}','{owner['id']}','own_review','{{}}');"
        f"INSERT INTO moderation_holds(case_id,target_kind,target_id,effect,starts_at) VALUES('{case_id}','account','{owner['id']}','restrict',clock_timestamp())"
    )
    current = f.publication(path, token=f.token(admin))
    f.publication(
        path + "/withdraw",
        method="POST",
        body={"expected_revision": current["visibility_revision"], "request_id": str(uuid4())},
        token=f.token(admin),
    )
    assert f.counters() == baseline
    f.publication(f"/auth/admin/users/{uuid4()}/publication", token=f.token(admin), expected=404)
    # Disabled recovery never opens the old/public paths or accepts support writes.
    f.publication_enabled = False
    f.restart()
    f.publication(path, token=f.token(admin), expected=503)
    f.publication(path + "/withdraw", method="POST", body=command, token=f.token(admin), expected=503)
    f.publication_enabled = True
    f.restart()
    audit = f.sql(f"SELECT count(*) FROM admin_audit_log WHERE path='{path}' AND method='GET' AND status=200")
    assert int(audit) >= 1


def support_queued_requests_recheck_current_authority(f):
    owner = f.account()
    _, share = f.preview_choice(owner)
    state = f.publication(method="PUT", body=share, token=f.token(owner))["current"]
    path = f"/auth/admin/users/{owner['id']}/publication/withdraw"
    before = f.sql(f"SELECT row_to_json(p)::text FROM user_profiles p WHERE user_id='{owner['id']}'")
    for restriction, expected in [("admin", 403), ("mfa", 403), ("session", 401)]:
        admin = support_admin(f)
        # Hold the same account lock as the request, then change authority before releasing it.
        with subprocess.Popen(
            [
                str(f.args.pg_bin / "psql"),
                "-XqAt",
                "-v",
                "ON_ERROR_STOP=1",
                "-h",
                "127.0.0.1",
                "-p",
                str(f.ports["pg"]),
                "-U",
                "safety",
                "-d",
                f.database,
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        ) as lock:
            try:
                lock.stdin.write(f"BEGIN; SELECT 1 FROM users WHERE id='{admin['id']}' FOR UPDATE;\n")
                lock.stdin.flush()
                assert lock.stdout.readline().strip() == "1"
                with ThreadPoolExecutor(max_workers=1) as executor:
                    pending = executor.submit(
                        f.publication,
                        path,
                        method="POST",
                        body={"expected_revision": state["visibility_revision"], "request_id": str(uuid4())},
                        token=f.token(admin),
                        expected=expected,
                    )
                    end = time.monotonic() + 10
                    while (
                        f.sql(
                            "SELECT count(*) FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE '%SELECT id FROM users%'"
                        )
                        == "0"
                    ):
                        assert time.monotonic() < end, "support request did not reach the real account lock"
                        time.sleep(0.03)
                    statement = {
                        "admin": f"UPDATE users SET admin=false WHERE id='{admin['id']}';",
                        "mfa": f"UPDATE sessions SET mfa_verified=false WHERE user_id='{admin['id']}';",
                        "session": f"DELETE FROM sessions WHERE user_id='{admin['id']}';",
                    }[restriction]
                    lock.stdin.write(statement + " COMMIT;\n")
                    lock.stdin.flush()
                    pending.result(timeout=20)
            finally:
                lock.stdin.close()
                lock.wait(timeout=10)
        assert f.sql(f"SELECT row_to_json(p)::text FROM user_profiles p WHERE user_id='{owner['id']}'") == before


REFUSED = {"detail": "owner_sign_in_required"}
PROFILE_ROW = "SELECT row_to_json(p)::text FROM user_profiles p WHERE user_id='{}'"


def impersonate(f, admin, owner):
    login = f.request(f"/auth/sessions/{owner['id']}", "POST", token=f.token(admin))
    assert login["user"]["id"] == owner["id"]
    assert login["session"]["device_name"] is None and login["session"]["mfa_verified"] is False
    stored = f.sql(f"SELECT origin||'|'||impersonated_by FROM sessions WHERE id='{login['session']['id']}'")
    assert stored == f"impersonation|{admin['id']}", stored
    return login


def refresh(f, login):
    renewed = f.request("/auth/session", "PUT", {"refresh_token": login["refresh_token"]})
    assert renewed["session"]["id"] == login["session"]["id"]
    return {**login, **renewed}


def withdrawal(revision):
    return {"profile_visibility": "private", "expected_revision": revision, "request_id": str(uuid4())}


def impersonation_cannot_choose_even_after_refresh(f):
    if not f.publication_enabled:
        f.activate()
    owner, admin = f.account(), support_admin(f)
    audit = (
        f"SELECT count(*) FROM admin_audit_log WHERE admin_user_id='{admin['id']}' "
        f"AND target_user_id='{owner['id']}' AND method='PUT' AND path='{OWNER}' AND status=403"
    )
    session = impersonate(f, admin, owner)
    before = f.sql(PROFILE_ROW.format(owner["id"]))
    # Troubleshooting may still read the choice and the preview.
    revision = f.publication(token=f.token(session))["visibility_revision"]
    preview = f.publication(OWNER + "-preview", token=f.token(session))
    share = {
        "profile_visibility": "shared",
        "expected_revision": revision,
        "scope_version": preview["scope_version"],
        "notice_hash": preview["notice_hash"],
        "preview_token": preview["preview_token"],
    }
    attempts = 0
    for step in ("issued", "refreshed", "restarted"):
        for body in (share, withdrawal(revision)):
            result = f.publication(
                method="PUT", body={**body, "request_id": str(uuid4())}, token=f.token(session), expected=403
            )
            assert result == REFUSED, (step, result)
            attempts += 1
        # The old opt-out flag shares or withdraws as well and is refused alike.
        f.request("/auth/users/me", "PATCH", {"leaderboard_opt_out": False}, token=f.token(session), expected=403)
        if step == "refreshed":
            f.restart()
        session = refresh(f, session)
    assert f.sql(PROFILE_ROW.format(owner["id"])) == before
    assert f.sql(f"SELECT origin FROM sessions WHERE id='{session['session']['id']}'") == "impersonation"
    assert owner["id"] not in [p["user_id"] for p in f.publication(ROOT + "snapshot", token=f.auth)["participants"]]
    # Every refused attempt is recorded for the administrator, acting on the account.
    assert int(f.sql(audit)) == attempts

    # The owner's own sign-in shares, also after a refresh, recorded as the owner's.
    owner = refresh(f, owner)
    _, own = f.preview_choice(owner)
    shared = f.publication(method="PUT", body=own, token=f.token(owner))
    assert shared["receipt"]["source"] == "owner"
    revision = shared["current"]["visibility_revision"]
    # The administrator's session cannot withdraw it either.
    assert f.publication(method="PUT", body=withdrawal(revision), token=f.token(session), expected=403) == REFUSED
    f.request("/auth/users/me", "PATCH", {"leaderboard_opt_out": True}, token=f.token(session), expected=403)
    assert f.publication(token=f.token(owner))["profile_visibility"] == "shared"
    assert int(f.sql(audit)) == attempts + 1
    # Support withdraws through its own route, recorded as support.
    withdrawn = f.publication(
        f"/auth/admin/users/{owner['id']}/publication/withdraw",
        method="POST",
        body={"expected_revision": revision, "request_id": str(uuid4())},
        token=f.token(admin),
    )
    assert withdrawn["receipt"]["source"] == "support"
    # The owner's own sign-in can withdraw too.
    _, again = f.preview_choice(owner)
    revision = f.publication(method="PUT", body=again, token=f.token(owner))["current"]["visibility_revision"]
    private = f.publication(method="PUT", body=withdrawal(revision), token=f.token(owner))
    assert private["current"]["profile_visibility"] == "private" and private["receipt"]["source"] == "owner"


def legacy_sessions_follow_their_device_name(f):
    if not f.publication_enabled:
        f.activate()
    owner, admin = f.account(), support_admin(f)
    # The fixture signs in with a User-Agent, like every browser.
    assert owner["session"]["device_name"]
    session = impersonate(f, admin, owner)
    ids = f"'{owner['session']['id']}','{session['session']['id']}'"
    # Sessions from before the migration carry no recorded origin.
    f.sql(f"UPDATE sessions SET origin='legacy', impersonated_by=NULL WHERE id IN ({ids})")
    owner, session = refresh(f, owner), refresh(f, session)
    assert f.sql(f"SELECT string_agg(origin, ',') FROM sessions WHERE id IN ({ids})") == "legacy,legacy"
    before = f.sql(PROFILE_ROW.format(owner["id"]))
    # Without a device name it may have been opened by an administrator.
    revision = f.publication(token=f.token(session))["visibility_revision"]
    _, share = f.preview_choice(session)
    assert f.publication(method="PUT", body=share, token=f.token(session), expected=403) == REFUSED
    assert f.publication(method="PUT", body=withdrawal(revision), token=f.token(session), expected=403) == REFUSED
    f.request("/auth/users/me", "PATCH", {"leaderboard_opt_out": False}, token=f.token(session), expected=403)
    assert f.sql(PROFILE_ROW.format(owner["id"])) == before
    # With a device name it is the owner's own sign-in.
    _, share = f.preview_choice(owner)
    shared = f.publication(method="PUT", body=share, token=f.token(owner))
    assert shared["receipt"]["source"] == "owner"
    private = f.publication(
        method="PUT", body=withdrawal(shared["current"]["visibility_revision"]), token=f.token(owner)
    )
    assert private["current"]["profile_visibility"] == "private"


CASES = [
    disabled_contract_and_service_auth,
    owner_preview_and_exact_projection,
    race_replay_and_legacy_clients,
    support_permissions_withdrawal_and_replay,
    support_queued_requests_recheck_current_authority,
    impersonation_cannot_choose_even_after_refresh,
    legacy_sessions_follow_their_device_name,
    reset_preserves_consent_and_revokes_publication_authority,
    queued_publication_cannot_gain_authority_after_reset,
    current_verification_withdrawal_and_export,
    moderation_boundaries_invalidate_cached_membership,
    erasure_removes_receipts_and_membership,
    scope_updates_invalidate_old_authority,
    dump_restore_and_disabled_recovery,
    authority_outage_is_closed,
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
    result = {
        "passed": False,
        "cases": [],
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip(),
        "scenario_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "binary_sha256": safety.binary_digest(args.binary),
    }
    try:
        with Fixture(args) as fixture:
            for case in CASES:
                start = time.monotonic()
                try:
                    case(fixture)
                except Exception as error:
                    result["cases"].append({"name": case.__name__, "passed": False, "error": str(error)})
                    raise
                result["cases"].append(
                    {"name": case.__name__, "passed": True, "seconds": round(time.monotonic() - start, 3)}
                )
                print(case.__name__ + " passed", flush=True)
        result["passed"] = True
    finally:
        result["fixture_removed"] = not fixture.base.exists() if "fixture" in locals() else None
        args.output.mkdir(parents=True, exist_ok=True)
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
