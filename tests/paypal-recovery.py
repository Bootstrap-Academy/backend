"""Synthetic HTTP/SMTP/PostgreSQL payment recovery regression.

Run after `cargo build -p academy` with Python 3 (standard library only).
Set T7_PG_BIN and T7_VALKEY_BIN to installed local executable directories/files.
Creates its own private cluster, fixtures and localhost-only services, and removes them.
T7_EVIDENCE_DIR optionally retains logs. Never uses an existing database/configuration.
"""

import base64
import concurrent.futures
import copy
import datetime
import hashlib
import hmac
import http.server
import json
import os
from pathlib import Path
import shutil
import socket
import socketserver
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/debug/academy"
PG = Path(os.environ.get("T7_PG_BIN", "/usr/bin"))
VALKEY = os.environ.get("T7_VALKEY_BIN", "valkey-server")
WORK = Path(tempfile.mkdtemp(prefix="academy-t7-"))
EVIDENCE = Path(os.environ.get("T7_EVIDENCE_DIR", str(WORK / "logs")))
EVIDENCE.mkdir(parents=True, exist_ok=True)


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


PGPORT, CACHEPORT, APIPORT = (free_port() for _ in range(3))
URL = f"postgresql://{os.environ['USER']}@127.0.0.1:{PGPORT}/t7"
BASE = f"http://127.0.0.1:{APIPORT}"
orders = {}
messages = []
rendered = []
controls = {"render_fail": False, "smtp_fail": False}
paid_event = threading.Event()
release_event = threading.Event()
smtp_event = threading.Event()
smtp_release = threading.Event()
backend = None
cache = None
pg_started = False


def sql(statement, fail=False):
    result = subprocess.run(
        [str(PG / "psql"), URL, "-XAt", "-v", "ON_ERROR_STOP=1", "-c", statement], capture_output=True, text=True
    )
    if fail:
        assert result.returncode != 0, statement
    else:
        assert result.returncode == 0, result.stderr
    return result.stdout.strip()


def row(order_id):
    return json.loads(sql(f"SELECT row_to_json(p) FROM paypal_payments p WHERE order_id='{order_id}'"))


def ledger(order_id):
    return int(sql(f"SELECT count(*) FROM transactions WHERE description='PayPal: {order_id}'"))


def req(path, method="GET", data=None, token=None):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    request = urllib.request.Request(
        BASE + path, data=None if data is None else json.dumps(data).encode(), method=method, headers=headers
    )
    try:
        response = urllib.request.urlopen(request, timeout=45)
    except urllib.error.HTTPError as error:
        response = error
    content = response.read()
    try:
        content = json.loads(content)
    except ValueError:
        pass
    return response.status, content


def token_for(uid):
    def encode(value):
        return base64.urlsafe_b64encode(json.dumps(value, separators=(",", ":")).encode()).rstrip(b"=")

    message = (
        encode({"alg": "HS256"})
        + b"."
        + encode(
            {
                "exp": int(time.time()) + 3600,
                "uid": uid,
                "sid": str(uuid.uuid4()),
                "rt": "00" * 32,
                "data": {"admin": False, "email_verified": True, "mfa": False},
            }
        )
    )
    signature = base64.urlsafe_b64encode(hmac.new(b"synthetic-t7", message, hashlib.sha256).digest()).rstrip(b"=")
    return (message + b"." + signature).decode()


class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def reply(self, status, payload):
        body = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_GET(self):
        assert self.headers.get("Authorization") == "Bearer synthetic-provider"
        order = orders[self.path.split("/")[-1]]
        if order.get("get_fail"):
            return self.reply(503, {})
        self.reply(200, order["remote"])

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        if self.path == "/v1/oauth2/token":
            assert self.headers.get("Authorization") == "Basic " + base64.b64encode(b"test-client:test-secret").decode()
            assert body == b"grant_type=client_credentials"
            return self.reply(200, {"access_token": "synthetic-provider", "token_type": "Bearer"})
        if self.path == "/html_to_pdf":
            rendered.append(body.decode())
            if controls["render_fail"]:
                return self.reply(503, {})
            # Synthetic bytes intentionally avoid requiring a browser or external renderer.
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"%PDF-SYNTHETIC\n" + body)
            return
        assert self.headers.get("Authorization") == "Bearer synthetic-provider"
        if self.path == "/v2/checkout/orders":
            data = json.loads(body)
            oid = uuid.uuid4().hex.upper()
            orders[oid] = {
                "remote": {
                    "id": oid,
                    "intent": "CAPTURE",
                    "status": "CREATED",
                    "purchase_units": [
                        {
                            "amount": data["purchase_units"][0]["amount"],
                            "payee": {"merchant_id": "SYNTHETICMERCHANT"},
                            "payments": {"captures": []},
                        }
                    ],
                },
                "calls": [],
                "charges": 0,
            }
            return self.reply(201, {"id": oid})
        oid = self.path.split("/")[-2]
        order = orders[oid]
        key = self.headers.get("PayPal-Request-Id")
        assert key and str(uuid.UUID(key)) == key
        assert self.headers.get("Prefer") == "return=representation"
        order["calls"].append(key)
        mode = order.get("mode")
        if mode == "fail_before":
            return self.reply(503, {})
        remote = order["remote"]
        unit = remote["purchase_units"][0]
        if not unit["payments"]["captures"]:
            remote["status"] = "COMPLETED"
            unit["payments"]["captures"] = [
                {
                    "id": "CAP" + oid,
                    "status": "PENDING" if mode == "pending" else "COMPLETED",
                    "amount": copy.deepcopy(unit["amount"]),
                    "create_time": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                }
            ]
            order["charges"] += 1
        if mode == "lost_unknown":
            order["get_fail"] = True
        if mode == "timeout":
            time.sleep(31)  # Exceeds the real provider adapter's 30-second request deadline.
        if mode == "crash":
            paid_event.set()
            release_event.wait(40)
        if mode in ("lost", "lost_unknown"):
            self.close_connection = True
            return
        if mode == "already":
            return self.reply(422, {"details": [{"issue": "ORDER_ALREADY_CAPTURED"}]})
        self.reply(201, remote)


class Smtp(socketserver.StreamRequestHandler):
    def handle(self):
        self.wfile.write(b"220 synthetic SMTP\r\n")
        while line := self.rfile.readline():
            verb = line.split(b" ", 1)[0].strip().upper()
            if verb in (b"EHLO", b"HELO"):
                self.wfile.write(b"250-localhost\r\n250 8BITMIME\r\n")
            elif verb == b"DATA":
                self.wfile.write(b"354 End with dot\r\n")
                data = b""
                while (line := self.rfile.readline()) not in (b".\r\n", b""):
                    data += line
                if controls["smtp_fail"]:
                    self.wfile.write(b"451 Temporary synthetic failure\r\n")
                else:
                    messages.append(data)
                    if controls.get("smtp_hold") and b"invoice-" in data:
                        smtp_event.set()
                        smtp_release.wait(40)
                    try:
                        self.wfile.write(b"250 Accepted\r\n")
                    except (BrokenPipeError, ConnectionResetError):
                        return
            elif verb == b"QUIT":
                self.wfile.write(b"221 Bye\r\n")
                return
            else:
                self.wfile.write(b"250 OK\r\n")


provider = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
smtp = socketserver.ThreadingTCPServer(("127.0.0.1", 0), Smtp)
smtp.daemon_threads = True
for service in (provider, smtp):
    threading.Thread(target=service.serve_forever, daemon=True).start()

CONFIG = WORK / "config.toml"
CONFIG.write_text(
    f"""[database]
url = "{URL}"
[http]
address = "127.0.0.1:{APIPORT}"
[cache]
url = "redis://127.0.0.1:{CACHEPORT}/0"
[email]
smtp_url = "smtp://127.0.0.1:{smtp.server_address[1]}"
from = "test@example.com"
[jwt]
secret = "synthetic-t7"
[paypal]
base_url_override = "http://127.0.0.1:{provider.server_address[1]}/"
client_id = "test-client"
client_secret = "test-secret"
[render]
daemon_url = "http://127.0.0.1:{provider.server_address[1]}/"
[finance]
invoices_archive = "{WORK}/invoices"
credit_notes_archive = "{WORK}/credits"
final_statements_archive = "{WORK}/final"
"""
)
ENV = os.environ | {"ACADEMY_CONFIG": f"{CONFIG}:{ROOT}/config.dev.toml", "RUST_LOG": "warn"}


def academy(*args, **kwargs):
    return subprocess.run([str(BIN), *args], env=ENV, cwd=ROOT, capture_output=True, text=True, **kwargs)


def start_backend():
    global backend
    backend = subprocess.Popen(
        [str(BIN), "serve"], env=ENV, cwd=ROOT, stdout=(EVIDENCE / "backend.log").open("a"), stderr=subprocess.STDOUT
    )
    for _ in range(100):
        try:
            req("/health")
            return
        except OSError:
            time.sleep(0.1)
    raise AssertionError("Backend failed to start: " + (EVIDENCE / "backend.log").read_text())


def restart_backend():
    backend.kill()
    backend.wait(10)
    start_backend()


def new_order(token, mode=None):
    status, quote = req("/shop/coins/paypal/offers/1337", "POST", token=token)
    assert status == 200, (status, quote)
    offer = quote["offer"]
    acceptance = {
        "order_id": offer["id"],
        "offer_hash": offer["hash"],
        "accepted": True,
        "early_performance_requested": True,
    }
    status, oid = req("/shop/coins/paypal/orders", "POST", {"coins": 1337, **acceptance}, token)
    assert status == 200, (status, oid)
    orders[oid]["remote"]["status"] = "APPROVED"
    orders[oid]["mode"] = mode
    assert row(oid)["started_at"] is None
    return oid


def capture(oid, token):
    return req(f"/shop/coins/paypal/orders/{oid}/capture", "POST", token=token)


def settled(oid):
    payment = row(oid)
    assert payment["capture_id"] == "CAP" + oid
    assert payment["fulfilled_at"] and payment["balance"] is not None
    assert ledger(oid) == 1
    assert orders[oid]["charges"] == 1
    assert len(set(orders[oid]["calls"])) == 1
    invoice = json.loads(
        sql(f"SELECT row_to_json(d) FROM financial_documents d WHERE number='R{payment['invoice_number']:07}'")
    )
    assert invoice["gross_total_cents"] == 1337
    assert invoice["net_total_cents"] + invoice["vat_total_cents"] == 1337
    return payment


def retry():
    result = academy("task", "retry-paypal-payments")
    assert result.returncode == 0, result.stderr


def commit_fault(oid, field):
    sql(
        f"""CREATE FUNCTION t7_commit_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
        IF NEW.order_id='{oid}' AND OLD.{field} IS NULL AND NEW.{field} IS NOT NULL THEN
            RAISE EXCEPTION 'synthetic deferred COMMIT failure'; END IF; RETURN NEW; END $$;
        CREATE CONSTRAINT TRIGGER t7_commit_fault AFTER UPDATE ON paypal_payments
        DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION t7_commit_fault();"""
    )


def clear_fault():
    sql("DROP TRIGGER t7_commit_fault ON paypal_payments; DROP FUNCTION t7_commit_fault();")


def wait_settled(oid):
    for _ in range(100):
        if row(oid)["receipt_sent_at"]:
            return settled(oid)
        time.sleep(0.1)
    raise AssertionError(row(oid))


try:
    subprocess.run(
        [str(PG / "initdb"), "-D", str(WORK / "pg"), "-A", "trust", "--no-locale"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    (WORK / "socket").mkdir()
    subprocess.run(
        [
            str(PG / "pg_ctl"),
            "-D",
            str(WORK / "pg"),
            "-l",
            str(EVIDENCE / "postgres.log"),
            "-o",
            f"-k {WORK}/socket -p {PGPORT} -h 127.0.0.1",
            "start",
        ],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    pg_started = True
    subprocess.run([str(PG / "createdb"), "-h", "127.0.0.1", "-p", str(PGPORT), "t7"], check=True)
    for args in (("migrate", "up"), ("migrate", "demo", "--force")):
        result = academy(*args)
        assert result.returncode == 0, result.stderr
    cache = subprocess.Popen(
        [VALKEY, "--bind", "127.0.0.1", "--port", str(CACHEPORT), "--save", "", "--appendonly", "no"],
        stdout=(EVIDENCE / "valkey.log").open("w"),
        stderr=subprocess.STDOUT,
    )
    start_backend()
    uid = sql("SELECT id FROM users WHERE name='foo'")
    token = token_for(uid)
    other = token_for(sql("SELECT id FROM users WHERE name='bar'"))
    status, body = req("/shop/coins/me", token=token)
    assert status == 200, (status, body)
    original_balance = body["coins"]
    oid = new_order(token)
    assert capture(oid, other)[0] == 404
    assert row(oid)["started_at"] is None and not orders[oid]["calls"]
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
        results = list(pool.map(lambda _: capture(oid, token), range(6)))
    assert all(status in (200, 503) for status, _ in results), results
    status, completed = capture(oid, token)
    if status != 200:
        print(
            "SYNTHETIC_DIAGNOSTIC",
            sql(
                f"SELECT jsonb_build_object('payment',to_jsonb(p),'offer',o.offer,'acceptance',to_jsonb(a),'progress',to_jsonb(g)) FROM paypal_payments p JOIN paypal_contract_orders c ON c.paypal_order_id=p.order_id JOIN purchase_offers o ON o.id=c.contract_order_id LEFT JOIN purchase_acceptances a ON a.order_id=o.id JOIN purchase_progress g ON g.order_id=o.id WHERE p.order_id='{oid}'"
            ),
            flush=True,
        )
    assert status == 200, completed
    assert all(body == completed for status, body in results if status == 200)
    assert completed["coins"] == original_balance + 1337
    assert len(orders[oid]["calls"]) == 1
    assert settled(oid)["receipt_sent_at"]
    print("PASS concurrent capture, ownership, stable replay, one credit/invoice/receipt", flush=True)

    for mode in ("lost", "already", "timeout", "fail_before", "pending", "lost_unknown"):
        oid = new_order(token, mode)
        status, _ = capture(oid, token)
        if mode in ("lost", "already", "timeout"):
            assert status == 200
        else:
            assert status == 503 and ledger(oid) == 0
            if mode == "pending":
                # An order marked COMPLETED still has a PENDING capture, so no fulfillment.
                assert row(oid)["capture_id"] is None
                orders[oid]["remote"]["purchase_units"][0]["payments"]["captures"][0]["status"] = "COMPLETED"
            orders[oid]["mode"] = None
            orders[oid]["get_fail"] = False
            assert capture(oid, token)[0] == 200
        settled(oid)
        print("PASS provider outcome", mode, flush=True)

    for field in ("capture_id", "fulfilled_at", "receipt_sent_at"):
        oid = new_order(token)
        commit_fault(oid, field)
        status, _ = capture(oid, token)
        payment = row(oid)
        assert payment[field] is None
        if field == "receipt_sent_at":
            assert status == 200 and ledger(oid) == 1
        else:
            assert status == 503 and ledger(oid) == 0
        if field == "fulfilled_at":
            assert payment["capture_id"] is not None
            assert sql(f"SELECT captured_at IS NULL FROM paypal_coin_orders WHERE id='{oid}'") == "t"
            assert (
                sql(f"SELECT count(*) FROM financial_documents WHERE number='R{payment['invoice_number']:07}'") == "0"
            )
        clear_fault()
        retry()
        assert settled(oid)["receipt_sent_at"]
        assert len(orders[oid]["calls"]) == 1
        print("PASS real deferred COMMIT rollback and recovery:", field, flush=True)

    controls["render_fail"] = True
    oid = new_order(token)
    assert capture(oid, token)[0] == 200
    payment = settled(oid)
    assert payment["receipt_sent_at"] is None and payment["receipt_attempts"] >= 1
    controls["render_fail"] = False
    retry()
    assert settled(oid)["receipt_sent_at"]
    print("PASS invoice render failure after committed credit, original recovery", flush=True)

    controls["smtp_fail"] = True
    oid = new_order(token)
    assert capture(oid, token)[0] == 503
    payment = row(oid)
    assert payment["capture_id"] and payment["balance"] is None and ledger(oid) == 0
    assert orders[oid]["charges"] == 1
    contract = json.loads(payment["snapshot"])["contract_order_id"]
    assert sql(f"SELECT state FROM purchase_progress WHERE order_id='{contract}'") == "paid"
    restart_backend()
    assert capture(oid, token)[0] == 503 and orders[oid]["charges"] == 1
    controls["smtp_fail"] = False
    sql(f"UPDATE purchase_progress SET next_attempt_at=now()-interval '1 second' WHERE order_id='{contract}'")
    assert capture(oid, token)[0] == 200
    assert settled(oid)["receipt_sent_at"] and orders[oid]["charges"] == 1
    print("PASS capture pending confirmation survives restart and same-order retry; one capture/credit", flush=True)

    # Snapshot regeneration keeps original recipient/address/VAT even with changed account/config.
    controls["render_fail"] = True
    oid = new_order(token)
    assert capture(oid, token)[0] == 200
    snapshot = json.loads(row(oid)["snapshot"])
    sql(f"UPDATE user_invoice_info SET first_name='CHANGED' WHERE user_id='{uid}'")
    CONFIG.write_text(CONFIG.read_text() + "vat_percent = 7\n")
    controls["render_fail"] = False
    restart_backend()
    payment = wait_settled(oid)
    pdf = (WORK / "invoices" / f"R{payment['invoice_number']:07}.pdf").read_text()
    assert "19 %" in pdf and "CHANGED" not in pdf
    assert json.loads(row(oid)["snapshot"]) == snapshot
    print("PASS restart receipt recovery preserves original customer and VAT facts", flush=True)

    # Actual process death after the provider moved funds but before its response was received.
    oid = new_order(token, "crash")
    with concurrent.futures.ThreadPoolExecutor() as pool:
        pending = pool.submit(capture, oid, token)
        assert paid_event.wait(10)
        backend.kill()
        backend.wait(10)
        release_event.set()
        try:
            pending.result(10)
        except OSError:
            pass
    assert row(oid)["started_at"] and not row(oid)["capture_id"]
    start_backend()
    wait_settled(oid)
    assert len(orders[oid]["calls"]) == 1
    print("PASS process death after provider capture, restart GET reconciliation, one credit", flush=True)

    # The credit COMMIT succeeded, but the customer loses the HTTP response during receipt work.
    controls["smtp_hold"] = True
    oid = new_order(token)
    with concurrent.futures.ThreadPoolExecutor() as pool:
        pending = pool.submit(capture, oid, token)
        assert smtp_event.wait(10)
        assert ledger(oid) == 1
        backend.kill()
        backend.wait(10)
        controls["smtp_hold"] = False
        smtp_release.set()
        try:
            pending.result(10)
        except OSError:
            pass
    # The first receipt artifact and its original invoice committed before the
    # lost handoff. Cache loss and renderer outage must not block replay.
    number = row(oid)["invoice_number"]
    original_pdf = bytes.fromhex(
        sql(f"SELECT encode(pdf,'hex') FROM invoice_originals WHERE invoice_number='R{number:07}'")
    )
    (WORK / "invoices" / f"R{number:07}.pdf").unlink()
    controls["render_fail"] = True
    start_backend()
    wait_settled(oid)
    status, download_token = req("/finance/token", token=token)
    assert status == 200
    status, downloaded = req(f"/finance/invoices/{download_token}/{number}/invoice.pdf")
    assert status == 200 and downloaded == original_pdf
    controls["render_fail"] = False
    assert capture(oid, token)[0] == 200 and ledger(oid) == 1
    assert len(orders[oid]["calls"]) == 1
    print("PASS lost customer response after accepted credit COMMIT; replay does not credit twice", flush=True)

    # Mismatches never authorize capture. Reconciliation can continue after corrected proof.
    for mismatch in ("id", "payee", "amount", "currency", "capture_amount", "capture_currency"):
        oid = new_order(token)
        remote = orders[oid]["remote"]
        original = copy.deepcopy(remote)
        unit = remote["purchase_units"][0]
        if mismatch == "id":
            remote["id"] = "WRONG"
        elif mismatch == "payee":
            unit["payee"]["merchant_id"] = "WRONG"
        elif mismatch in ("amount", "currency"):
            unit["amount"]["value" if mismatch == "amount" else "currency_code"] = (
                "1.00" if mismatch == "amount" else "USD"
            )
        else:
            remote["status"] = "COMPLETED"
            unit["payments"]["captures"] = [
                {
                    "id": "CAP" + oid,
                    "status": "COMPLETED",
                    "amount": {
                        "value": "1.00" if mismatch == "capture_amount" else "13.37",
                        "currency_code": "USD" if mismatch == "capture_currency" else "EUR",
                    },
                    "create_time": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                }
            ]
        assert capture(oid, token)[0] == 503
        assert ledger(oid) == 0 and not orders[oid]["calls"]
        assert row(oid)["last_error"] == "provider_evidence_mismatch"
        orders[oid]["remote"] = original
        assert capture(oid, token)[0] == 200
        settled(oid)
    print("PASS provider order/payee/amount/currency and capture amount/currency mismatch checks", flush=True)

    oid = new_order(token)
    sql(f"UPDATE paypal_payments SET started_at=now()-interval '6 hours' WHERE order_id='{oid}'")
    assert capture(oid, token)[0] == 503
    retry()
    assert ledger(oid) == 0 and orders[oid]["calls"] == []
    listing = academy("task", "list-paypal-payments")
    assert oid in listing.stdout and "provider_outcome_unresolved" in listing.stdout
    sql(f"UPDATE paypal_payments SET snapshot='{{}}' WHERE order_id='{oid}'", fail=True)
    sql(f"UPDATE paypal_payments SET started_at=now() WHERE order_id='{oid}'", fail=True)
    migration_down = ROOT / "academy_persistence/postgres/migrations/2026-09-07-180000_durable_paypal_payments/down.sql"
    sql("BEGIN;" + migration_down.read_text() + "COMMIT;", fail=True)
    print("PASS expired key is read-only, operator backlog, immutable evidence and downgrade guard", flush=True)

    # Missing recipient after uncertain capture retains the proven, unpaid claim.
    uid2 = sql("SELECT id FROM users WHERE name='admin'")
    sql(
        f"UPDATE user_invoice_info SET business=false,country='Germany',first_name='Synthetic',last_name='Recipient' WHERE user_id='{uid2}'"
    )
    token2 = token_for(uid2)
    oid2 = new_order(token2, "lost_unknown")
    assert capture(oid2, token2)[0] == 503
    sql(f"DELETE FROM users WHERE id='{uid2}'")
    orders[oid2]["get_fail"] = False
    retry()
    payment = row(oid2)
    assert payment["capture_id"] and payment["fulfilled_at"] is None
    assert payment["last_error"] == "recipient_missing_requires_settlement_review"
    assert payment["user_id"] == uid2 and ledger(oid2) == 0
    assert orders[oid2]["charges"] == 1
    print("PASS deleted-recipient captured claim survives without invented destination", flush=True)

    # Legacy ambiguity is reported and never submitted through a fresh capture key.
    sql(
        f"INSERT INTO paypal_coin_orders (id,user_id,coins,created_at,invoice_number) VALUES ('LEGACYT7','{uid}',1337,now(),nextval('invoice_number'))"
    )
    assert capture("LEGACYT7", token)[0] == 503
    listing = academy("task", "list-paypal-payments")
    assert "legacy_unresolved order=LEGACYT7" in listing.stdout
    print("PASS legacy unresolved order visible with no automatic capture", flush=True)

    number = int(sql("SELECT invoice_number FROM paypal_coin_orders WHERE id='LEGACYT7'"))
    before_transactions = sql("SELECT count(*) FROM transactions")
    status, download_token = req("/finance/token", token=token)
    assert status == 200
    assert req(f"/finance/invoices/{download_token}/{number}/invoice.pdf")[0] == 404
    assert sql(f"SELECT state FROM invoice_reconciliation WHERE invoice_number='R{number:07}'") == "evidence_missing"
    repair = academy("admin", "finance", "reconcile-invoices")
    assert repair.returncode == 0 and f"R{number:07}" in repair.stdout
    original = b"%PDF-1.4\nSynthetic retained original; capture UNKNOWN\n%%EOF"
    archived = WORK / "original.pdf"
    archived.write_bytes(original)
    repair = academy(
        "admin", "finance", "record-original-invoice", f"R{number:07}", str(archived), "synthetic-archive-reference"
    )
    assert repair.returncode == 0, repair.stderr
    assert req(f"/finance/invoices/{download_token}/{number}/invoice.pdf") == (200, original)
    assert sql("SELECT captured_at IS NULL FROM paypal_coin_orders WHERE id='LEGACYT7'") == "t"
    assert sql("SELECT count(*) FROM transactions") == before_transactions
    print(
        "PASS missing legacy PDF 404 commits repair obligation; archived original retrieval never invents capture or replays money",
        flush=True,
    )

    count = sql("SELECT count(*) FROM paypal_payments")
    print(
        f"PASS all recovery scenarios; {count} durable synthetic payment records; {len(messages)} SMTP acceptances",
        flush=True,
    )
finally:
    release_event.set()
    smtp_release.set()
    for process in (backend, cache):
        if process and process.poll() is None:
            process.terminate()
            try:
                process.wait(15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(5)
    for service in (provider, smtp):
        service.shutdown()
        service.server_close()
    if pg_started:
        subprocess.run(
            [str(PG / "pg_ctl"), "-D", str(WORK / "pg"), "-m", "immediate", "stop"],
            check=True,
            stdout=subprocess.DEVNULL,
        )
    shutil.rmtree(WORK)
    print("Cleanup: stopped synthetic services and removed private database/configuration/archives", flush=True)
