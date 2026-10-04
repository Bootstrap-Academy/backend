"""Native session-recheck races using the repository's owned PG/Valkey fixture.

python3 tests/sensitive-writes.py --binary /path/to/academy --pg-bin /path/to/bin \
    --valkey /path/to/valkey-server --output /path/to/new-evidence-directory

Every account, database, mailbox and provider is synthetic and local. The same
runner accepts the previously released executable and a candidate executable.
Use --case NAME to retain a focused counterexample. No external services or
additional Python packages are used; publication.py/backend-safety.py and the
existing purchase_provider.py are imported directly.
"""

import sys

# Test tools are themselves formatter inputs. Keep native runs from leaving
# compiled copies of the imported repository fixtures in tests/__pycache__.
sys.dont_write_bytecode = True

import argparse
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import selectors
import socket
import subprocess
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from uuid import uuid4

from purchase_provider import provider_handler


REPO = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("sensitive_publication", REPO / "tests/publication.py")
publication = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publication)
safety = publication.safety
RESET_CODE = "ABCD-EFGH-IJKL-MNOP"
RESET_PASSWORD = "session recheck reset password"
REDIRECT = "http://127.0.0.1:3000/oauth/callback"


class OAuthProvider(http.server.BaseHTTPRequestHandler):
    """The normal authorization/callback API talks to this loopback provider."""

    def log_message(self, *_args):
        pass

    def reply(self, payload):
        raw = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_POST(self):
        assert self.path == "/oauth2/token"
        body = urllib.parse.parse_qs(self.rfile.read(int(self.headers["Content-Length"])).decode())
        assert body["redirect_uri"] == [REDIRECT]
        assert body["grant_type"] == ["authorization_code"]
        assert body.get("code_verifier"), "normal PKCE exchange must be exercised"
        identity = body["code"][0]
        self.server.exchanges += 1
        self.reply({"access_token": identity, "token_type": "Bearer"})

    def do_GET(self):
        if self.path.startswith("/validate/"):
            return self.reply({"isValid": self.path == "/validate/DE/vat/987654321"})
        assert self.path == "/user"
        identity = self.headers["Authorization"].removeprefix("Bearer ")
        self.reply({"id": identity, "name": "Synthetic OAuth identity"})


class Fixture(publication.Fixture):
    pool_size = 10

    def __enter__(self):
        self.orders, self.rendered = {}, []
        self.controls = {"render_fail": False}
        self.paid, self.release = threading.Event(), threading.Event()
        self.provider = http.server.ThreadingHTTPServer(
            ("127.0.0.1", 0), provider_handler(self.orders, self.rendered, self.controls, self.paid, self.release)
        )
        self.server(self.provider)
        self.oauth = http.server.ThreadingHTTPServer(("127.0.0.1", 0), OAuthProvider)
        self.oauth.exchanges = 0
        self.server(self.oauth)
        return super().__enter__()

    def write_config(self):
        super().write_config()
        path = self.base / "fixture.toml"
        config = (
            path.read_text()
            .replace("[database]\n", f"[database]\nmax_connections = {self.pool_size}\n", 1)
            .replace('acquire_timeout = "2s"', 'acquire_timeout = "10s"')
        )
        provider = self.provider.server_address[1]
        config = config.replace(
            'base_url_override = "http://127.0.0.1:1/"', f'base_url_override = "http://127.0.0.1:{provider}/"'
        )
        config = config.replace('daemon_url = "http://127.0.0.1:1/"', f'daemon_url = "http://127.0.0.1:{provider}/"')
        config = config.replace(
            "[purchase.provision_window_seconds]\n", "[purchase.provision_window_seconds]\ncoins = 3600\n"
        )
        oauth = self.oauth.server_address[1]
        config = config.replace(
            'validate_endpoint_override = "http://127.0.0.1:1/"',
            f'validate_endpoint_override = "http://127.0.0.1:{oauth}/validate/"',
        )
        path.write_text(
            config
            + f"""
[oauth2.providers.test]
auth_url = "http://127.0.0.1:{oauth}/oauth2/authorize"
token_url = "http://127.0.0.1:{oauth}/oauth2/token"
userinfo_url = "http://127.0.0.1:{oauth}/user"
"""
        )

    def request(self, path, method="GET", body=None, token=None, expected=200):
        # Preserve empty and non-JSON replies, so an error remains an HTTP
        # observation instead of turning into a JSON-decoder fixture failure.
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
            response = urllib.request.urlopen(request, timeout=25)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            status, raw = response.status, response.read()
        try:
            data = json.loads(raw) if raw else None
        except json.JSONDecodeError:
            data = {"non_json_response_bytes": len(raw)}
        if expected is not None:
            assert status == expected, f"{method} {path}: expected {expected}, got {status}"
        return data if expected is not None else (status, data)

    def admin(self):
        account = self.account(admin=True, coins=200)
        secret = self.request("/auth/users/me/mfa", "POST", token=self.token(account))
        self.request("/auth/users/me/mfa", "PUT", {"code": safety.totp(secret)}, self.token(account))
        account.update(self.login(account["name"], account["password"], mfa_code=safety.totp(secret, 1)))
        assert account["session"]["mfa_verified"] is True
        return account

    def invoice(self, account):
        self.request(
            "/auth/users/me",
            "PATCH",
            {
                "business": False,
                "first_name": "Synthetic",
                "last_name": "Learner",
                "street": "Fixture street 1",
                "zip_code": "12345",
                "city": "Fixture",
                "country": "Germany",
            },
            self.token(account),
        )

    def oauth_callback(self, account):
        flow = self.request(
            "/auth/oauth/authorize", "POST", {"provider_id": "test", "redirect_uri": REDIRECT}, self.token(account)
        )
        return {"state": flow["state"], "code": "synthetic-" + uuid4().hex}

    def cash_quote(self, account):
        return self.request("/shop/coins/paypal/offers/1337", "POST", {}, self.token(account))

    def cash_create(self, account, quote):
        oid = self.request(
            "/shop/coins/paypal/orders", "POST", {"coins": 1337, **acceptance(quote)}, self.token(account)
        )
        # Model the buyer's explicit approval at the local provider. A merely
        # CREATED PayPal order is intentionally never captured by the backend.
        self.orders[oid]["remote"]["status"] = "APPROVED"
        return oid

    def state(self, account, *, sessions=False):
        """Observe effects without logging any password, bearer or MFA secret."""
        uid = account["id"]
        statement = f"""SELECT jsonb_build_object(
          'identity',(SELECT jsonb_build_object('name',name,'email',email,
            'email_verified',email_verified,'enabled',enabled,'admin',admin,
            'terms_version',terms_version,'terms_accepted_at',terms_accepted_at,
            'terms_declined_at',terms_declined_at) FROM users WHERE id='{uid}'),
          'profile',(SELECT to_jsonb(p) FROM user_profiles p WHERE user_id='{uid}'),
          'invoice',(SELECT to_jsonb(i) FROM user_invoice_info i WHERE user_id='{uid}'),
          'coins',(SELECT coins FROM coins WHERE user_id='{uid}'),
          'hearts',(SELECT to_jsonb(h) FROM hearts h WHERE user_id='{uid}'),
          'ledger_rows',(SELECT count(*) FROM transactions WHERE user_id='{uid}'),
          'premium',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM premium p WHERE user_id='{uid}'),
          'subscriptions',(SELECT jsonb_agg(to_jsonb(p)) FROM premium_subscriptions p WHERE user_id='{uid}'),
          'renewal_agreements',(SELECT count(*) FROM premium_renewal_agreements WHERE user_id='{uid}'),
          'renewal_cancellations',(SELECT count(*) FROM premium_renewal_cancellations c
            JOIN premium_renewal_agreements a ON a.id=c.agreement_id WHERE a.user_id='{uid}'),
          'mfa_devices',(SELECT jsonb_agg(to_jsonb(d) ORDER BY id) FROM totp_devices d WHERE user_id='{uid}'),
          'mfa_recovery_rows',(SELECT count(*) FROM mfa_recovery_codes WHERE user_id='{uid}'),
          'oauth_links',(SELECT jsonb_agg(to_jsonb(l) ORDER BY id) FROM oauth2_links l WHERE user_id='{uid}'),
          'purchase_offers',(SELECT count(*) FROM purchase_offers WHERE user_id='{uid}'),
          'purchase_submissions',(SELECT count(*) FROM purchase_submissions s JOIN purchase_offers o ON o.id=s.order_id WHERE o.user_id='{uid}'),
          'purchase_acceptances',(SELECT count(*) FROM purchase_acceptances a JOIN purchase_offers o ON o.id=a.order_id WHERE o.user_id='{uid}'),
          'purchase_debits',(SELECT count(*) FROM purchase_debits d JOIN purchase_offers o ON o.id=d.order_id WHERE o.user_id='{uid}'),
          'purchase_fulfillments',(SELECT count(*) FROM purchase_fulfillments f JOIN purchase_offers o ON o.id=f.order_id WHERE o.user_id='{uid}'),
          'purchase_states',(SELECT jsonb_agg(jsonb_build_object('id',p.order_id,'state',p.state) ORDER BY p.order_id)
            FROM purchase_progress p JOIN purchase_offers o ON o.id=p.order_id WHERE o.user_id='{uid}'),
          'paypal',(SELECT jsonb_agg(jsonb_build_object('id',order_id,'started_at',started_at,
            'capture_id',capture_id,'balance',balance,'fulfilled_at',fulfilled_at) ORDER BY order_id)
            FROM paypal_payments WHERE user_id='{uid}'),
          'deletion_work',(SELECT count(*) FROM user_deletion_work WHERE user_id='{uid}')
        )::text"""
        result = json.loads(self.sql(statement))
        result["peer_deletions"] = sum(deleted == uid for _, deleted in self.peer.deletions)
        result["provider_orders"] = len(self.orders)
        result["provider_captures"] = sum(order["charges"] for order in self.orders.values())
        result["oauth_exchanges"] = self.oauth.exchanges
        result["smtp_messages"] = self.smtp.messages
        if sessions:
            result["sessions"] = json.loads(
                self.sql(
                    f"SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'mfa_verified',mfa_verified) ORDER BY id),'[]'::jsonb)::text FROM sessions WHERE user_id='{uid}'"
                )
            )
        return result


def acceptance(quote):
    return {
        "order_id": quote["offer"]["id"],
        "offer_hash": quote["offer"]["hash"],
        "accepted": True,
        "early_performance_requested": True,
    }


def cache_set(f, key, payload):
    parts = [b"SET", key.encode(), payload, b"EX", b"600"]
    wire = b"*5\r\n" + b"".join(b"$" + str(len(part)).encode() + b"\r\n" + part + b"\r\n" for part in parts)
    with socket.create_connection(("127.0.0.1", f.ports["cache"]), timeout=5) as cache:
        cache.sendall(wire)
        assert cache.recv(1024) == b"+OK\r\n"


def seed_reset(f, account):
    # The cache service uses rmp-serde's compact tuple representation.
    def string(value):
        raw = value.encode()
        return (bytes([0xA0 + len(raw)]) if len(raw) < 32 else b"\xd9" + bytes([len(raw)])) + raw

    cache_set(
        f,
        "reset_password_code:v2:" + account["id"],
        b"\x92" + string(account["name"] + "@example.com") + string(RESET_CODE),
    )


def reset(f, account):
    return f.request(
        "/auth/password_reset",
        "PUT",
        {"email": account["name"] + "@example.com", "code": RESET_CODE, "password": RESET_PASSWORD},
        expected=None,
    )


class OwnerLock:
    def __init__(self, f, account):
        self.name = "sensitive-row-blocker-" + uuid4().hex
        env = {**f.env, "PGAPPNAME": self.name}
        self.process = subprocess.Popen(
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
            stderr=subprocess.PIPE,
            text=True,
            env=env,
        )
        self.process.stdin.write(f"BEGIN; SELECT id FROM users WHERE id='{account['id']}' FOR UPDATE;\n")
        self.process.stdin.flush()
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout, selectors.EVENT_READ)
            if not selector.select(timeout=10):
                self.close()
                raise RuntimeError("external PostgreSQL owner lock did not open")
        assert self.process.stdout.readline().strip() == account["id"]
        self.pid = int(f.sql(f"SELECT pid FROM pg_stat_activity WHERE application_name='{self.name}'"))

    def release(self):
        if self.process.poll() is None:
            self.process.stdin.write("COMMIT;\n\\q\n")
            self.process.stdin.flush()
            self.process.communicate(timeout=10)

    def close(self):
        if self.process.poll() is None:
            self.process.stdin.write("ROLLBACK;\n\\q\n")
            self.process.stdin.flush()
            try:
                self.process.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.communicate()


def blocked(f):
    raw = f.sql(
        """SELECT coalesce(jsonb_agg(jsonb_build_object('pid',pid,
        'event',wait_event,'query',query,'blocking_pids',pg_blocking_pids(pid)) ORDER BY pid),'[]')::text
        FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()
        AND wait_event_type='Lock' AND state='active'"""
    )
    return json.loads(raw)


def wait_blocked(f, future, excluded=()):
    deadline = time.monotonic() + 8
    last = []
    while time.monotonic() < deadline:
        last = blocked(f)
        found = [row for row in last if row["pid"] not in excluded]
        if found:
            return found
        if future.done():
            return []
        time.sleep(0.025)
    raise AssertionError(f"HTTP operation never reached an observed PostgreSQL lock; {len(last)} other waiters")


@dataclass
class Scenario:
    actor: dict
    target: dict
    path: str
    method: str = "POST"
    body: object = None
    positive_check: object = None

    def call(self, f):
        body = self.body() if callable(self.body) else self.body
        return f.request(self.path, self.method, body, f.token(self.actor), expected=None)


def purchase(f, kind, accept=False):
    account = f.account(coins=500)
    if kind == "hearts":
        f.heart_seed(account, 2)
    if accept:
        quote = f.quote(account, kind)
        return Scenario(
            account,
            account,
            "/shop/purchases/accept",
            body=acceptance(quote),
            positive_check=lambda value: value["state"] == "fulfilled",
        )
    return Scenario(account, account, "/shop/purchases/offers/" + kind, body={})


def cash(f, action):
    account = f.account(coins=500)
    f.invoice(account)
    if action == "offer":
        return Scenario(account, account, "/shop/coins/paypal/offers/1337", body={})
    quote = f.cash_quote(account)
    if action == "accept":
        return Scenario(account, account, "/shop/coins/paypal/orders", body={"coins": 1337, **acceptance(quote)})
    oid = f.cash_create(account, quote)
    return Scenario(account, account, f"/shop/coins/paypal/orders/{oid}/capture")


def patch(f, field, value, admin=False):
    actor = f.admin() if admin else f.account(coins=500)
    target = f.account(coins=500) if admin else actor
    if field == "vat_id":
        f.sql(f"UPDATE user_invoice_info SET business=true,vat_id='DE123456789' WHERE user_id='{target['id']}'")
    if field == "name":
        value = "changed" + uuid4().hex[:12]
    if field == "email":
        value = "changed-" + uuid4().hex + "@example.com"
    check = None
    if field == "password":
        check = lambda _value: f.login(target["name"], value)["user"]["id"] == target["id"]
    return Scenario(actor, target, "/auth/users/" + (target["id"] if admin else "me"), "PATCH", {field: value}, check)


def terms(f, decline=False):
    account = f.account(coins=500)
    if not decline:
        f.sql(f"UPDATE users SET terms_version='older-test-terms',terms_accepted_at=null WHERE id='{account['id']}'")
    return Scenario(
        account,
        account,
        "/auth/users/me/terms" + ("/decline" if decline else ""),
        body=None if decline else {"terms_version": "safety-test-terms", "age_confirmed": True},
    )


def deletion(f, admin=False):
    actor = f.admin() if admin else f.account(coins=500)
    target = f.account(coins=500) if admin else actor
    return Scenario(actor, target, "/auth/users/" + (target["id"] if admin else "me"), "DELETE")


def mfa(f, action):
    account = f.account(coins=500)
    secret = None
    if action in ("enable", "disable"):
        secret = f.request("/auth/users/me/mfa", "POST", token=f.token(account))
    if action == "disable":
        f.request("/auth/users/me/mfa", "PUT", {"code": safety.totp(secret)}, f.token(account))
    return Scenario(
        account,
        account,
        "/auth/users/me/mfa",
        {"initialize": "POST", "enable": "PUT", "disable": "DELETE"}[action],
        (lambda: {"code": safety.totp(secret)}) if action == "enable" else None,
    )


def oauth(f, action):
    account = f.account(coins=500)
    callback = f.oauth_callback(account)
    if action == "create":
        return Scenario(account, account, "/auth/oauth/links/me", body=callback)
    link = f.request("/auth/oauth/links/me", "POST", callback, f.token(account))
    return Scenario(account, account, "/auth/oauth/links/me/" + link["id"], "DELETE")


def coins(f):
    actor, target = f.admin(), f.account(coins=500)
    return Scenario(
        actor,
        target,
        "/shop/coins/" + target["id"],
        body={"coins": -17, "description": "Synthetic admin debit", "credit_note": False},
    )


def renewal(f, action):
    account = f.account(coins=500)
    f.accept(account, f.quote(account, "premium_monthly"))
    offer = f.request("/shop/premium/renewal-offer/me", token=f.token(account))
    consent = {
        "plan": "MONTHLY",
        "consent": {"request_id": str(uuid4()), "offer_id": offer["id"], "accepted": True, "withdrawal_consent": True},
    }
    if action in ("cancel", "debit"):
        f.request("/shop/premium/autopay", "PUT", consent, f.token(account))
    if action == "debit":
        f.sql(
            f"UPDATE premium SET since=now()-interval '2 months',until=now()-interval '1 day' WHERE user_id='{account['id']}'"
        )
        return Scenario(account, account, "/shop/premium/me", "GET")
    return Scenario(account, account, "/shop/premium/autopay", "PUT", consent if action == "enable" else {"plan": None})


def session(f, action, admin=False):
    actor = f.admin() if admin or action == "impersonate" else f.account(coins=500)
    target = f.account(coins=500) if admin or action == "impersonate" else actor
    if action == "impersonate":
        return Scenario(actor, target, "/auth/sessions/" + target["id"])
    if action == "current":
        return Scenario(actor, actor, "/auth/session", "DELETE")
    other = f.login(target["name"], target["password"])
    path = "/auth/sessions/" + (target["id"] if admin else "me")
    if action == "specific":
        path += "/" + other["session"]["id"]
    return Scenario(actor, target, path, "DELETE")


def verification(f, admin=False):
    actor = f.admin() if admin else f.account(coins=500)
    target = f.account(coins=500) if admin else actor
    f.sql(f"UPDATE users SET email_verified=false WHERE id='{target['id']}'")
    return Scenario(actor, target, "/auth/users/" + (target["id"] if admin else "me") + "/email")


PREPARE = {
    "purchase_offer": lambda f: purchase(f, "premium_monthly"),
    "purchase_accept": lambda f: purchase(f, "premium_monthly", True),
    "heart_purchase_accept": lambda f: purchase(f, "hearts", True),
    "cash_offer": lambda f: cash(f, "offer"),
    "cash_accept_create_intent": lambda f: cash(f, "accept"),
    "paypal_capture_intent": lambda f: cash(f, "capture"),
    "terms_accept": lambda f: terms(f),
    "terms_decline": lambda f: terms(f, True),
    "deletion_intake": lambda f: deletion(f),
    "mfa_initialize": lambda f: mfa(f, "initialize"),
    "mfa_enable": lambda f: mfa(f, "enable"),
    "mfa_disable": lambda f: mfa(f, "disable"),
    "oauth_create": lambda f: oauth(f, "create"),
    "oauth_delete": lambda f: oauth(f, "delete"),
    "admin_coin_deduction": coins,
    "renewal_enable": lambda f: renewal(f, "enable"),
    "renewal_cancel": lambda f: renewal(f, "cancel"),
    "renewal_get_debit": lambda f: renewal(f, "debit"),
    "impersonate": lambda f: session(f, "impersonate"),
    "delete_current_session": lambda f: session(f, "current"),
    "delete_specific_session": lambda f: session(f, "specific"),
    "delete_all_sessions": lambda f: session(f, "all"),
    "admin_delete_specific_session": lambda f: session(f, "specific", True),
    "admin_delete_all_sessions": lambda f: session(f, "all", True),
    "verification_request": lambda f: verification(f),
    "admin_verification_request": lambda f: verification(f, True),
}
for field, value in {
    "display_name": "Changed synthetic display name",
    "description": "Changed synthetic biography",
    "tags": ["fixture"],
    "leaderboard_opt_out": True,
    "name": "",
    "email": "",
    "password": "changed synthetic password",
    "business": False,
    "first_name": "Changed",
    "last_name": "Synthetic",
    "street": "Fixture street 2",
    "zip_code": "54321",
    "city": "Changed fixture city",
    "country": "Germany",
    "vat_id": "DE987654321",
}.items():
    PREPARE["patch_" + field] = lambda f, field=field, value=value: patch(f, field, value)
for field, value in {"admin": True, "email_verified": False}.items():
    PREPARE["admin_patch_" + field] = lambda f, field=field, value=value: patch(f, field, value, True)


def positive(f, prepare):
    scenario = prepare(f)
    before = f.state(scenario.target, sessions=True)
    status, body = scenario.call(f)
    assert status == 200, f"valid {scenario.method} {scenario.path} returned {status}"
    after = f.state(scenario.target, sessions=True)
    if scenario.positive_check is not None:
        assert scenario.positive_check(body), "valid request did not have its required result"
    else:
        assert before != after, "valid request returned 200 without the expected durable/local effect"
    return {"status": status, "effect_changed": before != after, "required_result_observed": True}


def queued_reset(f, prepare):
    scenario = prepare(f)
    seed_reset(f, scenario.actor)
    observe_sessions = scenario.actor["id"] != scenario.target["id"]
    before = f.state(scenario.target, sessions=observe_sessions)
    blocker = OwnerLock(f, scenario.actor)
    try:
        with ThreadPoolExecutor(max_workers=2) as pool:
            recovery = pool.submit(reset, f, scenario.actor)
            first = wait_blocked(f, recovery)
            assert first, "reset must wait behind the external account row lock"
            operation = pool.submit(scenario.call, f)
            second = wait_blocked(f, operation, {row["pid"] for row in first})
            blocked_before_release = not operation.done()
            queue_order_proved = any(blocker.pid in row["blocking_pids"] for row in first) and any(
                set(row["blocking_pids"]) & {entry["pid"] for entry in first} for row in second
            )
            blocker.release()
            reset_status = recovery.result(timeout=25)[0]
            status, body = operation.result(timeout=25)
        after = f.state(scenario.target, sessions=observe_sessions)
        old_status = f.request("/auth/session", token=f.token(scenario.actor), expected=None)[0]
        return {
            "passed": bool(
                first
                and second
                and queue_order_proved
                and blocked_before_release
                and reset_status == 200
                and status == 401
                and before == after
                and old_status == 401
            ),
            "reset_status": reset_status,
            "request_status": status,
            "request_state": body.get("state") if isinstance(body, dict) else None,
            "request_blocked_before_release": blocked_before_release,
            "reset_then_sensitive_lock_queue_proved": queue_order_proved,
            "reset_waiters": first,
            "sensitive_waiters": second,
            "old_access_status": old_status,
            "effects_unchanged": before == after,
            "changed_effect_keys": sorted(key for key in before if before[key] != after[key]),
            "effects_before": summary(before),
            "effects_after": summary(after),
            "before_sha256": digest(before),
            "after_sha256": digest(after),
        }
    finally:
        blocker.close()


def admin_demotion(f):
    actor, supervisor, target = f.admin(), f.admin(), f.account(coins=200)
    blocker = OwnerLock(f, actor)
    before = f.state(target, sessions=True)
    try:
        with ThreadPoolExecutor(max_workers=2) as pool:
            demotion = pool.submit(
                f.request, "/auth/users/" + actor["id"], "PATCH", {"admin": False}, f.token(supervisor), None
            )
            first = wait_blocked(f, demotion)
            assert first, "admin demotion must first queue on the actor lock"
            debit = pool.submit(
                f.request,
                "/shop/coins/" + target["id"],
                "POST",
                {"coins": -17, "credit_note": False},
                f.token(actor),
                None,
            )
            second = wait_blocked(f, debit, {row["pid"] for row in first})
            blocker.release()
            demotion_status, debit_status = demotion.result(timeout=25)[0], debit.result(timeout=25)[0]
        # The cache invalidation can reject the old token with 401, or the
        # durable role recheck can reject with 403. Both deny the adjustment.
        after = f.state(target, sessions=True)
        return {
            "passed": bool(
                first and second and demotion_status == 200 and debit_status in (401, 403) and before == after
            ),
            "demotion_status": demotion_status,
            "debit_status": debit_status,
            "effects_unchanged": before == after,
            "demotion_waiters": first,
            "debit_waiters": second,
        }
    finally:
        blocker.close()


def admin_target_wait_then_revoke(f, *, demote=False):
    target, actor = sorted((f.admin(), f.admin()), key=lambda user: user["id"])
    supervisor = f.admin() if demote else None
    seed_reset(f, actor)
    # The smaller target UUID is locked first by the ordered cross-account
    # handler. Its caller can therefore be revoked while the handler waits,
    # giving a second independent proof of the caller recheck after that wait.
    blocker = OwnerLock(f, target)
    before = f.state(target, sessions=True)
    try:
        with ThreadPoolExecutor(max_workers=1) as pool:
            debit = pool.submit(
                f.request,
                "/shop/coins/" + target["id"],
                "POST",
                {"coins": -17, "description": "Synthetic target-wait debit", "credit_note": False},
                f.token(actor),
                None,
            )
            waits = wait_blocked(f, debit)
            if demote:
                revoke_status = f.request(
                    "/auth/users/" + actor["id"], "PATCH", {"admin": False}, f.token(supervisor), expected=None
                )[0]
            else:
                revoke_status = reset(f, actor)[0]
            blocker.release()
            debit_status = debit.result(timeout=25)[0]
        after = f.state(target, sessions=True)
        expected = (401, 403) if demote else (401,)
        return {
            "passed": bool(waits and revoke_status == 200 and debit_status in expected and before == after),
            "revoke_kind": "admin_demotion" if demote else "password_reset",
            "revoke_status": revoke_status,
            "queued_debit_status": debit_status,
            "effects_unchanged": before == after,
            "target_waiters": waits,
            "coins_before": before["coins"],
            "coins_after": after["coins"],
        }
    finally:
        blocker.close()


def reciprocal_admin_writes(f):
    a, b = f.admin(), f.admin()
    # Both cross-account writes begin while the same first ordered UUID is
    # blocked, then compete for the same two actor/target account locks.
    blocker = OwnerLock(f, min((a, b), key=lambda user: user["id"]))
    try:
        with ThreadPoolExecutor(max_workers=2) as pool:
            one = pool.submit(
                f.request, "/auth/users/" + b["id"], "PATCH", {"display_name": "From admin A"}, f.token(a), None
            )
            first = wait_blocked(f, one)
            two = pool.submit(
                f.request, "/auth/users/" + a["id"], "PATCH", {"display_name": "From admin B"}, f.token(b), None
            )
            second = wait_blocked(f, two, {row["pid"] for row in first})
            blocker.release()
            statuses = [one.result(timeout=25)[0], two.result(timeout=25)[0]]
        names = [f.sql(f"SELECT display_name FROM user_profiles WHERE user_id='{user['id']}'") for user in (a, b)]
        return {
            "passed": bool(first and second and statuses == [200, 200] and names == ["From admin B", "From admin A"]),
            "statuses": statuses,
            "durable_names_correct": names == ["From admin B", "From admin A"],
            "first_waiters": first,
            "second_waiters": second,
        }
    finally:
        blocker.close()


def purchase_replays_survive_reset(f):
    account = f.account(coins=500)
    quote = f.quote(account, "premium_monthly")
    with ThreadPoolExecutor(max_workers=10) as pool:
        responses = list(pool.map(lambda _: f.accept(account, quote), range(10)))
    assert all(
        value["state"] in ("accepted", "paid", "fulfilled") for value in responses
    ), "concurrent accepted retries must retain an accepted/paid/fulfilled state"
    assert f.accept(account, quote)["state"] == "fulfilled", "the accepted order must finish fulfillment"
    before = f.state(account)
    assert before["coins"] == 400 and before["ledger_rows"] == 1, "ten accepted retries must debit exactly once"
    assert (
        before["purchase_debits"] == 1 and before["purchase_fulfillments"] == 1
    ), "ten accepted retries must fulfill exactly once"
    seed_reset(f, account)
    assert reset(f, account)[0] == 200
    denied_status = f.accept(account, quote, expected=None)[0]
    assert denied_status in (401, 404), "the revoked token must not access the completed order"
    assert f.state(account) == before, "reset must preserve already fulfilled obligations"
    account.update(f.login(account["name"], RESET_PASSWORD))
    retry = f.accept(account, quote)
    after = f.state(account)
    assert retry["state"] == "fulfilled" and after == before
    return {
        "passed": True,
        "simultaneous_valid_retries": 10,
        "coins_after": after["coins"],
        "ledger_rows": after["ledger_rows"],
        "fulfillments": after["purchase_fulfillments"],
        "concurrent_response_states": [value["state"] for value in responses],
        "revoked_fulfilled_order_status": denied_status,
        "fulfilled_purchase_preserved_after_reset_and_fresh_retry": True,
    }


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def summary(state):
    return {
        key: state[key]
        for key in (
            "coins",
            "ledger_rows",
            "purchase_offers",
            "purchase_states",
            "purchase_submissions",
            "purchase_acceptances",
            "purchase_debits",
            "purchase_fulfillments",
            "renewal_agreements",
            "renewal_cancellations",
            "deletion_work",
            "peer_deletions",
            "provider_orders",
            "provider_captures",
            "oauth_exchanges",
            "smtp_messages",
        )
    }


def closed_admin_routes(f):
    actor, target = f.admin(), f.account(coins=200)
    before = f.state(target, sessions=True)
    statuses = [
        f.request("/auth/users/" + target["id"], "PATCH", {"enabled": False}, f.token(actor), None)[0],
        f.request("/auth/users/" + target["id"], "DELETE", token=f.token(actor), expected=None)[0],
    ]
    after = f.state(target, sessions=True)
    return {
        "passed": statuses == [409, 409] and before == after,
        "statuses": statuses,
        "effects_unchanged": before == after,
        "reason": "Direct administrative disabling/deletion require the existing moderation flow",
    }


def paypal_obligation_survives_reset(f):
    account = f.account(coins=500)
    f.invoice(account)
    quote = f.cash_quote(account)
    oid = f.cash_create(account, quote)
    # The local provider pauses after its one synthetic charge. The durable
    # capture intent has already committed, so reset cannot cancel settlement.
    f.orders[oid]["mode"] = "crash"
    f.paid.clear()
    f.release.clear()
    try:
        with ThreadPoolExecutor(max_workers=1) as pool:
            capture = pool.submit(
                f.request, f"/shop/coins/paypal/orders/{oid}/capture", "POST", None, f.token(account), None
            )
            assert f.paid.wait(timeout=10), "capture did not reach the isolated provider"
            seed_reset(f, account)
            reset_status = reset(f, account)[0]
            f.release.set()
            capture_status = capture.result(timeout=25)[0]
        row = f.state(account)
        assert reset_status == 200 and capture_status == 200
        assert row["coins"] == 1837 and f.orders[oid]["charges"] == 1
        assert row["paypal"][0]["fulfilled_at"] is not None
        assert row["purchase_fulfillments"] == 1
        f.request(f"/shop/coins/paypal/orders/{oid}/capture", "POST", token=f.token(account), expected=401)
        account.update(f.login(account["name"], RESET_PASSWORD))
        f.request(f"/shop/coins/paypal/orders/{oid}/capture", "POST", token=f.token(account))
        assert f.state(account) == row and f.orders[oid]["charges"] == 1
        return {
            "passed": True,
            "reset_status": reset_status,
            "capture_status": capture_status,
            "synthetic_provider_charges": 1,
            "coins_after": row["coins"],
            "authorized_capture_preserved_and_fresh_retry_does_not_recharge": True,
        }
    finally:
        f.release.set()


def paypal_create_after_acceptance_reset(f):
    account = f.account(coins=500)
    f.invoice(account)
    quote = f.cash_quote(account)
    seed_reset(f, account)
    before = f.state(account)
    blocker = OwnerLock(f, account)
    try:
        with ThreadPoolExecutor(max_workers=2) as pool:
            creation = pool.submit(
                f.request,
                "/shop/coins/paypal/orders",
                "POST",
                {"coins": 1337, **acceptance(quote)},
                f.token(account),
                None,
            )
            acceptance_waiters = wait_blocked(f, creation)
            assert acceptance_waiters, "cash acceptance must first queue on the owner lock"
            recovery = pool.submit(reset, f, account)
            reset_waiters = wait_blocked(f, recovery, {row["pid"] for row in acceptance_waiters})
            queue_proved = any(blocker.pid in row["blocking_pids"] for row in acceptance_waiters) and any(
                set(row["blocking_pids"]) & {entry["pid"] for entry in acceptance_waiters} for row in reset_waiters
            )
            blocker.release()
            create_status, reset_status = creation.result(timeout=25)[0], recovery.result(timeout=25)[0]
        after = f.state(account)
        retained_acceptance = after["purchase_acceptances"] == before["purchase_acceptances"] + 1
        no_payment_created = (
            after["paypal"] == before["paypal"] and after["provider_orders"] == before["provider_orders"]
        )
        return {
            "passed": queue_proved
            and reset_status == 200
            and create_status == 401
            and retained_acceptance
            and no_payment_created
            and after["coins"] == before["coins"],
            "creation_status": create_status,
            "reset_status": reset_status,
            "cash_acceptance_then_reset_queue_proved": queue_proved,
            "accepted_contract_preserved": retained_acceptance,
            "no_provider_or_payment_order_created_after_reset": no_payment_created,
            "acceptance_waiters": acceptance_waiters,
            "reset_waiters": reset_waiters,
            "effects_before": summary(before),
            "effects_after": summary(after),
        }
    finally:
        blocker.close()


def paypal_capture_and_recovery_queue(f):
    account = f.account(coins=500)
    f.invoice(account)
    quote = f.cash_quote(account)
    oid = f.cash_create(account, quote)
    f.orders[oid]["mode"] = "crash"
    f.paid.clear()
    f.release.clear()
    try:
        with ThreadPoolExecutor(max_workers=3) as pool:
            first = pool.submit(
                f.request, f"/shop/coins/paypal/orders/{oid}/capture", "POST", None, f.token(account), None
            )
            assert f.paid.wait(timeout=10), "first capture did not reach the paused local provider"
            second = pool.submit(
                f.request, f"/shop/coins/paypal/orders/{oid}/capture", "POST", None, f.token(account), None
            )
            second_waiters = wait_blocked(f, second)
            recovery = pool.submit(
                f.command, f.args.binary, "task", "retry-paypal-payments", label="concurrent-paypal-recovery"
            )
            recovery_waiters = wait_blocked(f, recovery, {row["pid"] for row in second_waiters})
            f.release.set()
            replies = [first.result(timeout=25), second.result(timeout=25)]
            recovery.result(timeout=25)
        statuses = [status for status, _ in replies]
        # Confirmation delivery may still be in flight in another processor.
        # The API explicitly exposes that state as PaymentPendingError. Require
        # the original order to replay successfully after recovery completes.
        pending_code = (
            "Your payment is still being checked. Please retry this order later; " "do not place a second order."
        )
        capture_replies_valid = all(
            status == 200 or (status == 503 and body == {"code": pending_code}) for status, body in replies
        )
        replay_status, _ = f.request(f"/shop/coins/paypal/orders/{oid}/capture", "POST", None, f.token(account), None)
        state = f.state(account)
        charges = f.orders[oid]["charges"]
        return {
            "passed": bool(
                second_waiters
                and recovery_waiters
                and capture_replies_valid
                and replay_status == 200
                and charges == 1
                and state["coins"] == 1837
                and state["ledger_rows"] == 1
                and state["purchase_fulfillments"] == 1
            ),
            "capture_statuses": statuses,
            "capture_replies_match_success_or_documented_pending": capture_replies_valid,
            "original_order_replay_after_recovery_status": replay_status,
            "native_recovery_completed": True,
            "second_capture_waiters": second_waiters,
            "recovery_waiters": recovery_waiters,
            "synthetic_provider_charges": charges,
            "coins_after": state["coins"],
            "ledger_rows": state["ledger_rows"],
            "fulfillments": state["purchase_fulfillments"],
        }
    finally:
        f.release.set()


def one_connection_healthy_queue(f):
    account = f.account(coins=500)
    f.pool_size = 1
    f.restart()
    blocker = OwnerLock(f, account)
    try:
        with ThreadPoolExecutor(max_workers=10) as pool:
            calls = [
                pool.submit(
                    f.request,
                    "/auth/users/me",
                    "PATCH",
                    {"display_name": f"Healthy queued {index}"},
                    f.token(account),
                    None,
                )
                for index in range(10)
            ]
            first = wait_blocked(f, calls[0])
            peak = int(
                f.sql(
                    f"SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND application_name<>'{blocker.name}' AND backend_type='client backend'"
                )
            )
            blocker.release()
            statuses = [call.result(timeout=25)[0] for call in calls]
        return {
            "passed": bool(first and peak == 1 and statuses == [200] * 10),
            "configured_pool_size": 1,
            "observed_backend_connections_while_blocked": peak,
            "healthy_simultaneous_requests": 10,
            "statuses": statuses,
            "observed_waiters": first,
        }
    finally:
        blocker.close()
        f.pool_size = 10
        f.restart()


EXTRA = {
    "closed_admin_routes": closed_admin_routes,
    "admin_caller_demotion": admin_demotion,
    "admin_target_wait_caller_reset": admin_target_wait_then_revoke,
    "admin_target_wait_caller_demotion": lambda f: admin_target_wait_then_revoke(f, demote=True),
    "reciprocal_admin_writes": reciprocal_admin_writes,
    "purchase_replays_survive_reset": purchase_replays_survive_reset,
    "paypal_obligation_survives_reset": paypal_obligation_survives_reset,
    "paypal_create_after_acceptance_reset": paypal_create_after_acceptance_reset,
    "paypal_capture_and_recovery_queue": paypal_capture_and_recovery_queue,
    "one_connection_healthy_queue": one_connection_healthy_queue,
}


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--pg-bin", type=Path, required=True)
    parser.add_argument("--valkey", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--case", action="append", choices=sorted({*PREPARE, *EXTRA}))
    args = parser.parse_args()
    args.binary, args.pg_bin, args.valkey, args.output = (
        path.resolve() for path in (args.binary, args.pg_bin, args.valkey, args.output)
    )
    assert not args.output.exists(), "choose a new evidence directory; previous results are retained"
    args.output.mkdir(parents=True)
    result = {
        "schema_version": 1,
        "binary": str(args.binary),
        "binary_sha256": safety.binary_digest(args.binary),
        "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "reused_fixture_sha256": {
            name: hashlib.sha256((REPO / "tests" / name).read_bytes()).hexdigest()
            for name in ("publication.py", "backend-safety.py", "purchase_provider.py")
        },
        "postgres_bin": str(args.pg_bin),
        "valkey": str(args.valkey),
        "selected_cases": args.case or list(PREPARE) + list(EXTRA),
        "checks": [],
        "scope": "owned loopback HTTP/PostgreSQL/Valkey/SMTP; synthetic accounts/providers only",
        "core_only_paths": ["HeartFeatureService.refill", "PremiumFeatureService.purchase"],
    }

    def record(name, function):
        start = time.monotonic()
        try:
            check = {"name": name, **function()}
            check.setdefault("passed", True)
        except BaseException as error:
            check = {"name": name, "passed": False, "error": f"{type(error).__name__}: {error}"}
        check["seconds"] = round(time.monotonic() - start, 3)
        result["checks"].append(check)
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print(("PASS " if check["passed"] else "FAIL ") + name, flush=True)

    try:
        with Fixture(args) as fixture:
            for name, prepare in PREPARE.items():
                if args.case and name not in args.case:
                    continue
                record(name + "/valid", lambda prepare=prepare: positive(fixture, prepare))
                record(name + "/queued_reset", lambda prepare=prepare: queued_reset(fixture, prepare))
            for name, function in EXTRA.items():
                if not args.case or name in args.case:
                    record(name, lambda function=function: function(fixture))
        result["cleanup"] = "owned backend, PostgreSQL, Valkey, SMTP, providers and temporary cluster stopped/removed"
    except BaseException as error:
        result["fixture_error"] = f"{type(error).__name__}: {error}"
    result["passed"] = (
        bool(result["checks"]) and all(check["passed"] for check in result["checks"]) and "fixture_error" not in result
    )
    (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    raise SystemExit(0 if result["passed"] else 1)


if __name__ == "__main__":
    main()
