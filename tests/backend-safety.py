"""Native account/wallet regressions in owned loopback PostgreSQL and Valkey.

python3 tests/backend-safety.py --binary target/debug/academy --pg-bin /path/to/bin \
    --valkey /path/to/valkey-server --output /tmp/backend-safety-evidence

Only Python's standard library is needed. No existing database, deployed config,
provider, or mailbox is used. Each scenario creates its own synthetic accounts.
"""

import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import hmac
import http.server
import json
import os
from pathlib import Path
import socket
import socketserver
import struct
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request
from uuid import uuid4


REPO = Path(__file__).resolve().parents[1]


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def totp(secret, offset=0):
    secret = base64.b32decode(secret + "=" * (-len(secret) % 8))
    counter = int(time.time()) // 30 + offset
    digest = hmac.new(secret, struct.pack(">Q", counter), hashlib.sha1).digest()
    index = digest[-1] & 15
    return f"{(struct.unpack('>I', digest[index:index + 4])[0] & 0x7fffffff) % 1000000:06d}"


class SMTP(socketserver.StreamRequestHandler):
    def handle(self):
        self.wfile.write(b"220 isolated test SMTP\r\n")
        while line := self.rfile.readline():
            verb = line.split(b" ", 1)[0].strip().upper()
            if verb in (b"EHLO", b"HELO"):
                self.wfile.write(b"250-localhost\r\n250 8BITMIME\r\n")
            elif verb == b"DATA":
                self.wfile.write(b"354 send data\r\n")
                while self.rfile.readline() not in (b".\r\n", b""):
                    pass
                # No message body, recipient or attachment is saved.
                self.server.messages += 1
                self.wfile.write(b"250 accepted\r\n")
            elif verb == b"QUIT":
                self.wfile.write(b"221 bye\r\n")
                return
            else:
                self.wfile.write(b"250 OK\r\n")


class SMTPServer(socketserver.ThreadingTCPServer):
    daemon_threads = True


class Peer(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def respond(self, status, value):
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        parts = self.path.split("/")
        service = parts[1]
        if service in self.server.unavailable:
            self.respond(503, {"detail": "synthetic unavailable service"})
        else:
            self.respond(200, {"owner": parts[-2], "service": service})

    def do_DELETE(self):
        parts = self.path.split("/")
        self.server.deletions.append((parts[1], parts[-1]))
        self.respond(503 if parts[1] in self.server.unavailable else 200, {})


class Fixture:
    def __init__(self, args):
        self.args = args
        self.output = args.output.resolve()
        self.output.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix="academy-backend-safety-")
        self.base = Path(self.temp.name)
        self.processes = []
        self.logs = []
        self.servers = []
        self.pg_started = False
        self.backend = None
        self.ports = {name: free_port() for name in ("pg", "cache", "api")}
        self.env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("PG") and key not in ("ACADEMY_CONFIG", "DATABASE_URL")
        }
        self.env["RUST_LOG"] = "error"
        self.env["ACADEMY_CONFIG"] = f"{self.base}/fixture.toml:{REPO}/config.dev.toml"
        self.policy = 'mode = "legacy"'

    def command(self, *args, label=None):
        result = subprocess.run(
            [str(arg) for arg in args], cwd=REPO, env=self.env, capture_output=True, text=True, check=False
        )
        if label:
            (self.output / f"{label}.log").write_text(result.stdout + result.stderr)
        if result.returncode:
            # Commands never carry customer data or live credentials. Retain
            # SQL diagnostics for fixture mistakes without printing JWT output.
            diagnostic = result.stderr.splitlines()[0] if result.stderr else "no diagnostic"
            raise RuntimeError(f"{label or 'fixture command'} failed: {diagnostic}")
        return result.stdout.strip()

    def spawn(self, name, args):
        log = (self.output / f"{name}.log").open("ab")
        self.logs.append(log)
        process = subprocess.Popen(
            [str(arg) for arg in args], cwd=REPO, env=self.env, stdout=log, stderr=subprocess.STDOUT
        )
        self.processes.append(process)
        return process

    def server(self, server):
        self.servers.append(server)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        server.thread = thread
        return server.server_address[1]

    def write_config(self):
        api, pg, cache = (self.ports[name] for name in ("api", "pg", "cache"))
        peer = self.peer.server_address[1]
        smtp = self.smtp.server_address[1]
        (self.base / "fixture.toml").write_text(
            f"""
[http]
address = "127.0.0.1:{api}"
[database]
url = "postgresql://safety@127.0.0.1:{pg}/safety"
[cache]
url = "redis://127.0.0.1:{cache}/0"
[email]
smtp_url = "smtp://127.0.0.1:{smtp}"
[jwt]
secret = "isolated-backend-safety-secret"
[internal.secrets]
shop = "isolated-shop-secret"
auth = "isolated-auth-secret"
[user]
terms_version = "safety-test-terms"
export_rate_limit = "1h"
[session]
login_fails_before_lock = 100
login_fails_per_ip = 1000
[heart]
max = 10
refill_price = 17
auto_refill_time = "00:00:00"
[premium]
monthly_price = 100
yearly_price = 1000
[purchase.provision_window_seconds]
hearts = 86400
premium_monthly = 86400
premium_yearly = 86400
[microservices]
skills_url = "http://127.0.0.1:{peer}/skills/"
challenges_url = "http://127.0.0.1:{peer}/challenges/"
events_url = "http://127.0.0.1:{peer}/events/"
timeout = "2s"
export_timeout = "2s"
[paypal]
base_url_override = "http://127.0.0.1:1/"
[vat]
validate_endpoint_override = "http://127.0.0.1:1/"
[render]
daemon_url = "http://127.0.0.1:1/"
[finance]
invoices_archive = "{self.base}/invoices"
credit_notes_archive = "{self.base}/credits"
final_statements_archive = "{self.base}/statements"
[learning_policy]
{self.policy}
"""
        )

    def __enter__(self):
        try:
            self.smtp = SMTPServer(("127.0.0.1", 0), SMTP)
            self.smtp.messages = 0
            self.server(self.smtp)
            self.peer = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Peer)
            self.peer.unavailable = set()
            self.peer.deletions = []
            self.server(self.peer)
            self.write_config()
            pg = self.args.pg_bin
            self.command(
                pg / "initdb",
                "-D",
                self.base / "pg",
                "-U",
                "safety",
                "--auth=trust",
                "--no-locale",
                "--encoding=UTF8",
                label="initdb",
            )
            with (self.base / "pg/postgresql.conf").open("a") as handle:
                handle.write(
                    f"\nlisten_addresses='127.0.0.1'\nport={self.ports['pg']}\n"
                    f"unix_socket_directories='{self.base}'\n"
                )
            self.pg_started = True
            self.command(
                pg / "pg_ctl",
                "-D",
                self.base / "pg",
                "-l",
                self.output / "postgres.log",
                "-w",
                "start",
                label="postgres-start",
            )
            self.command(
                pg / "createdb", "-h", "127.0.0.1", "-p", self.ports["pg"], "-U", "safety", "safety", label="createdb"
            )
            cache = self.spawn(
                "valkey",
                [
                    self.args.valkey,
                    "--bind",
                    "127.0.0.1",
                    "--port",
                    self.ports["cache"],
                    "--save",
                    "",
                    "--appendonly",
                    "no",
                ],
            )
            self.wait_port("cache", cache)
            self.command(self.args.binary, "migrate", "up", label="migrate")
            self.start_backend()
            self.shop = self.command(self.args.binary, "jwt", "sign", '{"aud":"shop"}')
            self.auth = self.command(self.args.binary, "jwt", "sign", '{"aud":"auth"}')
            return self
        except BaseException:
            self.close()
            raise

    def wait_port(self, name, process):
        until = time.monotonic() + 30
        while time.monotonic() < until:
            if process.poll() is not None:
                raise RuntimeError(f"{name} failed to start; inspect its fixture log")
            try:
                with socket.create_connection(("127.0.0.1", self.ports[name]), timeout=0.2):
                    return
            except OSError:
                time.sleep(0.05)
        raise RuntimeError(f"{name} did not open its fixture port")

    def start_backend(self):
        self.backend = self.spawn("backend", [self.args.binary, "serve"])
        self.wait_port("api", self.backend)

    def restart(self, policy=None):
        self.backend.terminate()
        self.backend.wait(timeout=15)
        if policy is not None:
            self.policy = policy
        self.write_config()
        self.start_backend()

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
            "safety",
            "-c",
            statement,
        )

    def request(self, path, method="GET", body=None, token=None, expected=200):
        headers = {"Content-Type": "application/json"}
        if token:
            headers["Authorization"] = f"Bearer {token}"
        request = urllib.request.Request(
            f"http://127.0.0.1:{self.ports['api']}{path}",
            method=method,
            headers=headers,
            data=None if body is None else json.dumps(body).encode(),
        )
        try:
            response = urllib.request.urlopen(request, timeout=15)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            status = response.status
            data = json.loads(response.read())
        if expected is not None:
            assert status == expected, f"{method} {path}: expected {expected}, got {status}"
        return data if expected is not None else (status, data)

    def account(self, *, admin=False, coins=0):
        name = "safety" + uuid4().hex[:12]
        password = "synthetic test password"
        login = self.request(
            "/auth/users",
            "POST",
            {
                "name": name,
                "display_name": name,
                "email": f"{name}@example.com",
                "password": password,
                "terms_version": "safety-test-terms",
                "age_confirmed": True,
            },
        )
        uid = login["user"]["id"]
        self.sql(
            f"UPDATE users SET email_verified=true, admin={str(admin).lower()} WHERE id='{uid}';"
            f"INSERT INTO coins(user_id,coins,withheld_coins) VALUES('{uid}',{coins},0)"
        )
        login = self.login(name, password)
        return {"id": uid, "name": name, "password": password, **login}

    def login(self, name, password, **mfa):
        return self.request("/auth/sessions", "POST", {"name_or_email": name, "password": password, **mfa})

    def token(self, account):
        return account["access_token"]

    def balance(self, account):
        return self.request("/shop/coins/me", token=self.token(account))["coins"]

    def heart_seed(self, account, hearts, stale=False):
        at = "now()-interval '2 days'" if stale else "now()"
        self.sql(
            f"INSERT INTO hearts(user_id,hearts,last_refill) VALUES('{account['id']}',{hearts},{at}) "
            "ON CONFLICT(user_id) DO UPDATE SET hearts=excluded.hearts,last_refill=excluded.last_refill"
        )

    def heart_operation(self, account, operation=None, **changes):
        operation = operation or str(uuid4())
        return self.request(
            f"/shop/_internal/heart-operations/{operation}/{account['id']}",
            "PUT",
            {"half_hearts": 2, "reason": "incorrect_challenge_attempt", **changes},
            self.shop,
        )

    def quote(self, account, kind):
        return self.request(f"/shop/purchases/offers/{kind}", "POST", {}, self.token(account))

    def accept(self, account, quote, expected=200):
        return self.request(
            "/shop/purchases/accept",
            "POST",
            {
                "order_id": quote["offer"]["id"],
                "offer_hash": quote["offer"]["hash"],
                "accepted": True,
                "early_performance_requested": True,
            },
            self.token(account),
            expected,
        )

    def close(self):
        for process in reversed(self.processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        for server in self.servers:
            server.shutdown()
            server.server_close()
            server.thread.join(timeout=5)
        if self.pg_started and (self.base / "pg/postmaster.pid").exists():
            self.command(
                self.args.pg_bin / "pg_ctl", "-D", self.base / "pg", "-m", "fast", "-w", "stop", label="postgres-stop"
            )
        for log in self.logs:
            log.close()
        self.temp.cleanup()

    def __exit__(self, *_args):
        self.close()


def concurrent(*calls):
    barrier = threading.Barrier(len(calls))

    def call(function):
        barrier.wait(timeout=10)
        return function()

    with ThreadPoolExecutor(max_workers=len(calls)) as executor:
        return list(executor.map(call, calls))


def hearts_replay_and_last_heart(f):
    user = f.account()
    f.heart_seed(user, 2)
    operation = str(uuid4())
    first, retry = concurrent(lambda: f.heart_operation(user, operation), lambda: f.heart_operation(user, operation))
    assert first == retry and first["charged_half_hearts"] == 2
    assert f.heart_operation(user)["outcome"] == "insufficient"
    assert f.sql(f"SELECT hearts FROM hearts WHERE user_id='{user['id']}'") == "0"
    assert f.sql(f"SELECT count(*) FROM internal_heart_operations WHERE user_id='{user['id']}'") == "2"
    f.heart_seed(user, 4)
    assert f.heart_operation(user, operation) == first
    assert f.sql(f"SELECT hearts FROM hearts WHERE user_id='{user['id']}'") == "4"
    f.request(
        f"/shop/_internal/heart-operations/{operation}/{user['id']}",
        "PUT",
        {"half_hearts": 2, "reason": "changed"},
        f.shop,
        expected=409,
    )
    new = f.account()
    f.heart_seed(new, 2)
    results = concurrent(lambda: f.heart_operation(new), lambda: f.heart_operation(new))
    assert sorted(r["outcome"] for r in results) == ["charged", "insufficient"]


def heart_reset_and_refill(f):
    user = f.account(coins=100)
    f.heart_seed(user, 0, stale=True)
    assert f.request("/shop/hearts/me", token=f.token(user))["hearts"] == 10
    f.heart_seed(user, 2)
    quote = f.quote(user, "hearts")
    concurrent(lambda: f.accept(user, quote), lambda: f.accept(user, quote))
    assert f.balance(user) == 83
    assert f.request("/shop/hearts/me", token=f.token(user))["hearts"] == 10
    f.heart_operation(user)
    f.accept(user, quote)
    assert f.balance(user) == 83
    assert f.request("/shop/hearts/me", token=f.token(user))["hearts"] == 8


def coins_idempotence_and_no_free_credit(f):
    user = f.account(coins=7)
    path = f"/shop/_internal/coin-operations/{uuid4()}/{user['id']}"
    debit = {"coins": -5, "description": "synthetic debit", "credit_note": False}
    first, retry = concurrent(
        lambda: f.request(path, "PUT", debit, f.shop), lambda: f.request(path, "PUT", debit, f.shop)
    )
    assert first == retry and f.balance(user) == 2
    f.request(path, "PUT", {**debit, "coins": -6}, f.shop, expected=409)
    f.request(f"/shop/_internal/coins/{user['id']}", "POST", {"coins": 1}, f.shop, expected=403)
    f.request(f"/shop/_internal/coins/{user['id']}", "POST", {"coins": -3}, f.shop, expected=412)
    assert f.balance(user) == 2
    assert f.sql(f"SELECT count(*) FROM transactions WHERE user_id='{user['id']}'") == "1"


def coin_receipt_and_ledger_rollback_together(f):
    user = f.account(coins=8)
    path = f"/shop/_internal/coin-operations/{uuid4()}/{user['id']}"
    debit = {"coins": -3, "description": "synthetic rollback", "credit_note": False}
    f.sql(
        f"""CREATE FUNCTION safety_ledger_fault() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN IF NEW.user_id='{user['id']}'::uuid THEN RAISE EXCEPTION 'synthetic ledger failure';
        END IF; RETURN NEW; END; $$;
        CREATE TRIGGER safety_ledger_fault BEFORE INSERT ON transactions
        FOR EACH ROW EXECUTE FUNCTION safety_ledger_fault();"""
    )
    try:
        f.request(path, "PUT", debit, f.shop, expected=500)
        assert f.balance(user) == 8
        assert f.sql(f"SELECT count(*) FROM internal_coin_operations WHERE user_id='{user['id']}'") == "0"
    finally:
        f.sql("DROP TRIGGER safety_ledger_fault ON transactions; DROP FUNCTION safety_ledger_fault()")
    assert f.request(path, "PUT", debit, f.shop)["coins"] == 5
    assert f.balance(user) == 5
    assert f.sql(f"SELECT count(*) FROM transactions WHERE user_id='{user['id']}'") == "1"


def premium_purchase_replay_and_expiry(f):
    user = f.account(coins=200)
    quote = f.quote(user, "premium_monthly")
    concurrent(lambda: f.accept(user, quote), lambda: f.accept(user, quote))
    assert f.balance(user) == 100
    assert f.request("/shop/premium/me", token=f.token(user))["premium"] is True
    assert f.sql(f"SELECT count(*) FROM premium WHERE user_id='{user['id']}'") == "1"
    f.sql(
        f"UPDATE premium SET since=now()-interval '2 months',until=now()-interval '1 day' "
        f"WHERE user_id='{user['id']}'"
    )
    # Reading policy cannot perform the separately authorized renewal.
    assert f.request("/shop/learning/policy", token=f.token(user))["premium"] is False
    assert f.request(f"/shop/_internal/learning-policy/{user['id']}", token=f.shop)["premium"] is False
    assert f.balance(user) == 100
    assert f.request("/shop/premium/me", token=f.token(user))["premium"] is False
    assert f.balance(user) == 100


def premium_confirmed_renewal_only(f):
    user = f.account(coins=500)
    f.accept(user, f.quote(user, "premium_monthly"))
    offer = f.request("/shop/premium/renewal-offer/me", token=f.token(user))
    f.request("/shop/premium/autopay", "PUT", {"plan": "MONTHLY"}, f.token(user), expected=412)
    f.request(
        "/shop/premium/autopay",
        "PUT",
        {
            "plan": "MONTHLY",
            "consent": {
                "request_id": str(uuid4()),
                "offer_id": offer["id"],
                "accepted": True,
                "withdrawal_consent": True,
            },
        },
        f.token(user),
    )
    f.sql(
        f"UPDATE premium SET since=now()-interval '2 months',until=now()-interval '1 day' "
        f"WHERE user_id='{user['id']}'"
    )
    # Pending funded renewal is deliberately unavailable, so learning cannot
    # classify a soon-to-renew subscriber as free. Policy never settles it.
    f.request("/shop/learning/policy", token=f.token(user), expected=500)
    f.request(f"/shop/_internal/learning-policy/{user['id']}", token=f.shop, expected=500)
    assert f.balance(user) == 400
    concurrent(
        lambda: f.request("/shop/premium/me", token=f.token(user)),
        lambda: f.request("/shop/premium/me", token=f.token(user)),
    )
    assert f.balance(user) == 300
    assert f.request("/shop/learning/policy", token=f.token(user))["premium"] is True
    f.request("/shop/premium/autopay", "PUT", {"plan": None}, f.token(user))
    assert f.request("/shop/premium/me", token=f.token(user))["premium"] is True
    assert f.balance(user) == 300


def premium_unconfirmed_and_unfunded_renewal(f):
    for confirmed in (False, True):
        user = f.account(coins=100)
        f.accept(user, f.quote(user, "premium_monthly"))
        offer = f.request("/shop/premium/renewal-offer/me", token=f.token(user))
        f.request(
            "/shop/premium/autopay",
            "PUT",
            {
                "plan": "MONTHLY",
                "consent": {
                    "request_id": str(uuid4()),
                    "offer_id": offer["id"],
                    "accepted": True,
                    "withdrawal_consent": True,
                },
            },
            f.token(user),
        )
        if not confirmed:
            # An owned fixture simulates delivery never having succeeded.
            f.sql(
                f"DELETE FROM premium_renewal_delivery WHERE agreement_id IN "
                f"(SELECT id FROM premium_renewal_agreements WHERE user_id='{user['id']}');"
                f"UPDATE coins SET coins=200 WHERE user_id='{user['id']}'"
            )
        f.sql(
            f"UPDATE premium SET since=now()-interval '2 months',until=now()-interval '1 day' "
            f"WHERE user_id='{user['id']}'"
        )
        status = f.request("/shop/premium/me", token=f.token(user))
        assert status["premium"] is False and status["autopay"] is None
        assert f.balance(user) == (0 if confirmed else 200)
        assert f.sql(f"SELECT count(*) FROM transactions WHERE user_id='{user['id']}'") == "1"


def policy_auth_and_read_only(f):
    user = f.account(coins=100)
    public = f.request("/shop/learning/policy", token=f.token(user))
    internal = f.request(f"/shop/_internal/learning-policy/{user['id']}", token=f.shop)
    assert public == internal and public["mode"] == "legacy"
    f.request("/shop/learning/policy", expected=401)
    f.request(f"/shop/_internal/learning-policy/{user['id']}", token=f.auth, expected=401)
    f.request(f"/shop/_internal/learning-policy/{uuid4()}", token=f.shop, expected=404)
    assert f.balance(user) == 100
    assert f.sql(f"SELECT count(*) FROM internal_heart_operations WHERE user_id='{user['id']}'") == "0"


def policy_cohorts_preserve_legacy_and_daily_is_free(f):
    enrolled = f.account()
    legacy = f.account()
    for user in (enrolled, legacy):
        f.heart_seed(user, 6)
    f.restart('mode = "shadow"')
    assert f.request("/shop/learning/policy", token=f.token(legacy))["mode"] == "shadow"
    assert f.heart_operation(legacy)["charged_half_hearts"] == 2
    # These synthetic bytes exist only to configure an isolated Daily fixture.
    # The test makes no claim about legal documents or their delivery.
    document = f.base / "synthetic.pdf"
    document.write_bytes(b"%PDF-1.4 synthetic safety fixture\n")
    digest = hashlib.sha256(document.read_bytes()).hexdigest()
    policy = f'''mode = "daily"
terms_version = "safety-test-terms"
accepted_since = "2000-01-01T00:00:00Z"
user_ids = ["{enrolled['id']}"]
[learning_policy.daily_documents]
terms_version = "safety-test-terms"
terms_pdf_path = "{document}"
terms_sha256 = "{digest}"
withdrawal_pdf_path = "{document}"
withdrawal_sha256 = "{digest}"'''
    f.restart(policy)
    try:
        public = f.request("/shop/learning/policy", token=f.token(enrolled))
        internal = f.request(f"/shop/_internal/learning-policy/{enrolled['id']}", token=f.shop)
        assert public == internal and public["mode"] == "daily"
        assert public["heart_sales"] is False and public["single_course_sales"] is False
        operation = str(uuid4())
        receipt = f.heart_operation(enrolled, operation)
        assert receipt["outcome"] == "daily_learning" and receipt["charged_half_hearts"] == 0
        assert f.heart_operation(enrolled, operation) == receipt
        assert f.request("/shop/hearts/me", token=f.token(enrolled))["hearts"] == 6
        assert f.request("/shop/learning/policy", token=f.token(legacy))["mode"] == "legacy"
        assert f.heart_operation(legacy)["charged_half_hearts"] == 2
        f.sql(f"UPDATE users SET terms_version='previous-test-terms' WHERE id='{enrolled['id']}'")
        assert f.request("/shop/learning/policy", token=f.token(enrolled))["mode"] == "legacy"
        assert f.heart_operation(enrolled)["charged_half_hearts"] == 2
    finally:
        f.restart('mode = "legacy"')


def refresh_logout_and_owner_boundaries(f):
    user = f.account()
    other_session = f.login(user["name"], user["password"])

    def renew():
        return f.request("/auth/session", "PUT", {"refresh_token": user["refresh_token"]}, expected=None)

    results = concurrent(renew, renew)
    assert sorted(status for status, _ in results) == [200, 401]
    winner = next(login for status, login in results if status == 200)
    f.request("/auth/session", token=user["access_token"], expected=401)
    f.request("/auth/session", token=winner["access_token"])
    stranger = f.account()
    f.request(f"/auth/sessions/{user['id']}/{winner['session']['id']}", "DELETE", token=f.token(stranger), expected=403)
    f.request("/auth/session", "DELETE", token=winner["access_token"])
    f.request("/auth/session", token=winner["access_token"], expected=401)
    f.request("/auth/session", "PUT", {"refresh_token": winner["refresh_token"]}, expected=401)
    f.request("/auth/session", token=other_session["access_token"])


def mfa_setup_replay_and_recovery(f):
    user = f.account(admin=True)
    f.request("/auth/users", token=f.token(user), expected=403)
    secret = f.request("/auth/users/me/mfa", "POST", token=f.token(user))
    code = totp(secret)
    recovery = f.request("/auth/users/me/mfa", "PUT", {"code": code}, f.token(user))
    f.request("/auth/sessions", "POST", {"name_or_email": user["name"], "password": user["password"]}, expected=412)
    f.request(
        "/auth/sessions",
        "POST",
        {"name_or_email": user["name"], "password": user["password"], "mfa_code": code},
        expected=412,
    )
    verified = f.login(user["name"], user["password"], mfa_code=totp(secret, offset=1))
    assert verified["session"]["mfa_verified"] is True
    f.request("/auth/users", token=verified["access_token"])
    recovered = f.login(user["name"], user["password"], recovery_code=recovery)
    assert recovered["session"]["mfa_verified"] is False
    f.request("/auth/users", token=recovered["access_token"], expected=403)
    assert f.sql(f"SELECT count(*) FROM mfa_recovery_codes WHERE user_id='{user['id']}'") == "0"
    f.request("/auth/users", token=verified["access_token"], expected=401)


def export_isolated_partial_and_rate_limited(f):
    user = f.account(coins=13)
    stranger = f.account(coins=99)
    f.heart_operation(user)
    f.request(f"/auth/users/{user['id']}/export", token=f.token(stranger), expected=403)
    f.peer.unavailable.add("events")
    try:
        exported = f.request("/auth/users/me/export", token=f.token(user))
    finally:
        f.peer.unavailable.clear()
    assert exported["complete"] is False
    assert exported["account"]["user"]["id"] == user["id"]
    assert exported["account"]["balance"]["coins"] == 13
    assert len(exported["account"]["heart_operations"]) == 1
    assert exported["services"]["events"] == {"available": False, "data": None}
    assert exported["services"]["skills"]["data"]["owner"] == user["id"]
    body = json.dumps(exported)
    assert user["access_token"] not in body and user["refresh_token"] not in body
    assert stranger["id"] not in body and "password_hash" not in body
    f.request("/auth/users/me/export", token=f.token(user), expected=429)


def deletion_revokes_all_sessions_and_keeps_durable_work(f):
    user = f.account()
    another = f.login(user["name"], user["password"])
    stranger = f.account()
    f.heart_operation(user)
    f.request(f"/auth/users/{user['id']}", "DELETE", token=f.token(stranger), expected=403)
    f.peer.unavailable.add("skills")
    try:
        f.request("/auth/users/me", "DELETE", token=f.token(user))
    finally:
        f.peer.unavailable.clear()
    for session in (user, another):
        f.request("/auth/session", token=session["access_token"], expected=401)
        f.request("/auth/session", "PUT", {"refresh_token": session["refresh_token"]}, expected=401)
    for table in ("users", "sessions", "hearts", "internal_heart_operations"):
        column = "id" if table == "users" else "user_id"
        assert f.sql(f"SELECT count(*) FROM {table} WHERE {column}='{user['id']}'") == "0"
    assert int(f.sql(f"SELECT count(*) FROM user_deletion_work WHERE user_id='{user['id']}'")) > 0
    f.request("/auth/session", token=f.token(stranger))
    f.restart()
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        pending = f.sql(f"SELECT count(*) FROM user_deletion_work WHERE user_id='{user['id']}'")
        if pending == "0":
            break
        time.sleep(0.1)
    assert pending == "0", "deletion delivery did not recover after restart"


CASES = [
    hearts_replay_and_last_heart,
    heart_reset_and_refill,
    coins_idempotence_and_no_free_credit,
    coin_receipt_and_ledger_rollback_together,
    premium_purchase_replay_and_expiry,
    premium_confirmed_renewal_only,
    premium_unconfirmed_and_unfunded_renewal,
    policy_auth_and_read_only,
    policy_cohorts_preserve_legacy_and_daily_is_free,
    refresh_logout_and_owner_boundaries,
    mfa_setup_replay_and_recovery,
    export_isolated_partial_and_rate_limited,
    deletion_revokes_all_sessions_and_keeps_durable_work,
]


def binary_digest(path):
    with path.open("rb") as binary:
        return hashlib.file_digest(binary, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--pg-bin", type=Path, required=True)
    parser.add_argument("--valkey", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--case", help="Run one named scenario")
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error("run as an unprivileged user")
    for name in ("binary", "pg_bin", "valkey"):
        setattr(args, name, getattr(args, name).resolve())
    for name in ("initdb", "pg_ctl", "createdb", "psql"):
        version = subprocess.check_output([args.pg_bin / name, "--version"], text=True)
        if "(PostgreSQL) 18." not in version:
            parser.error(f"{name} must come from PostgreSQL 18")
    selected = [case for case in CASES if args.case is None or case.__name__ == args.case]
    if not selected:
        parser.error("unknown scenario")
    started = time.monotonic()
    result = {
        "passed": False,
        "cases": [],
        "source": str(REPO),
        "scenario_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip(),
        "binary_sha256": binary_digest(args.binary),
    }
    try:
        with Fixture(args) as fixture:
            for case in selected:
                case_started = time.monotonic()
                try:
                    case(fixture)
                except Exception as error:
                    result["cases"].append(
                        {
                            "name": case.__name__,
                            "passed": False,
                            "error": str(error),
                            "seconds": time.monotonic() - case_started,
                        }
                    )
                    print(f"FAIL {case.__name__}: {error}", flush=True)
                else:
                    result["cases"].append(
                        {"name": case.__name__, "passed": True, "seconds": time.monotonic() - case_started}
                    )
                    print(f"PASS {case.__name__}", flush=True)
            result["isolated_smtp_messages"] = fixture.smtp.messages
        result["fixture_removed"] = not fixture.base.exists()
        result["passed"] = all(case["passed"] for case in result["cases"])
    finally:
        result["seconds"] = time.monotonic() - started
        args.output.mkdir(parents=True, exist_ok=True)
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    raise SystemExit(0 if result["passed"] else 1)


if __name__ == "__main__":
    main()
