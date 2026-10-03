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


CASES = [
    disabled_contract_and_service_auth,
    owner_preview_and_exact_projection,
    race_replay_and_legacy_clients,
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
