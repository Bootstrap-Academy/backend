"""Confirmation deadlines against the backend and database of the isolated Nix VM."""

import datetime as dt
import json
from email import policy
from email.parser import BytesParser
from pathlib import Path
import socketserver
import subprocess
import threading
import time
import urllib.request
import uuid

from utils import configure_purchases

# This file is copied and run only by nix/tests/default.nix in its disposable VM.
# Reuse that VM's configured PostgreSQL, Valkey, backend and renderer. The SMTP
# fault boundary applies only to renewal confirmations, not one-off purchases.
assert Path(__file__).resolve().parent == Path("/root/tests")
assert subprocess.check_output(["hostname"], text=True).strip() == "machine"
configure_purchases()
CONFIG = Path("/run/academy-backend/secrets.toml")
ORIGINAL_CONFIG = CONFIG.read_bytes()
BINARY = "academy"


def run(args, **kw):
    return subprocess.run(args, check=True, text=True, **kw)


def sql(statement):
    return run(
        ["sudo", "-u", "postgres", "psql", "academy", "-XqAt", "-v", "ON_ERROR_STOP=1", "-c", statement],
        capture_output=True,
    ).stdout.strip()


assert sql("select current_database()") == "academy"


def request(method, path, body=None, token=None):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    req = urllib.request.Request(
        "http://127.0.0.1:8000" + path, json.dumps(body).encode() if body is not None else None, headers, method=method
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)


def restart_backend():
    run(["systemctl", "restart", "academy-backend.service"])
    for _ in range(100):
        try:
            request("GET", "/health")
            return
        except Exception:
            time.sleep(0.1)
    raise AssertionError("VM backend did not become ready after restart")


class SMTP(socketserver.StreamRequestHandler):
    delay = 0
    reject = False
    accepted = []
    started = threading.Event()

    def write(self, text):
        self.wfile.write(text.encode() + b"\r\n")
        self.wfile.flush()

    def handle(self):
        self.write("220 local review sink")
        while line := self.rfile.readline():
            command = line.decode().strip().upper()
            if command.startswith(("EHLO", "HELO")):
                self.write("250-localhost\r\n250 8BITMIME")
            elif command == "DATA":
                self.write("354 Send data")
                message = []
                while (line := self.rfile.readline()) != b".\r\n":
                    if not line:
                        return
                    message.append(line)
                parsed = BytesParser(policy=policy.default).parsebytes(b"".join(message))
                renewal = "monatliche Premium-Verlängerung" in str(parsed["Subject"])
                if renewal:
                    type(self).started.set()
                    time.sleep(type(self).delay)
                if renewal and type(self).reject:
                    self.write("451 Controlled rejection")
                else:
                    if renewal:
                        type(self).accepted.append(dt.datetime.now(dt.timezone.utc))
                    self.write("250 Accepted")
            elif command == "QUIT":
                self.write("221 Bye")
                return
            else:
                self.write("250 OK")


def user(name):
    run(
        [BINARY, "admin", "user", "create", "--verified", name, name + "@example.com", "test-password"],
        stdout=subprocess.DEVNULL,
    )
    login = request("POST", "/auth/sessions", {"name_or_email": name, "password": "test-password"})
    user_id = login["user"]["id"]
    token = login["access_token"]
    run([BINARY, "admin", "coin", "add", user_id, "--", "50000"], stdout=subprocess.DEVNULL)
    purchase(token)
    return user_id, token


def purchase(token, observe_status=True):
    offer = request("POST", "/shop/purchases/offers/premium_monthly", token=token)
    accepted = request(
        "POST",
        "/shop/purchases/accept",
        {
            "order_id": offer["offer"]["id"],
            "offer_hash": offer["offer"]["hash"],
            "accepted": True,
            "early_performance_requested": True,
        },
        token,
    )
    assert accepted["state"] == "fulfilled", accepted
    assert accepted["confirmation_smtp_accepted_at"] is not None
    if not observe_status:
        return accepted
    return request("GET", "/shop/premium/me", token=token)


def enable(token, request_id=None):
    offer = request("GET", "/shop/premium/renewal-offer")
    request_id = request_id or str(uuid.uuid4())
    request(
        "PUT",
        "/shop/premium/autopay",
        {
            "plan": "MONTHLY",
            "consent": {
                "request_id": request_id,
                "offer_id": offer["id"],
                "accepted": True,
                "withdrawal_consent": True,
            },
        },
        token,
    )
    return request_id


def balance(token):
    return request("GET", "/shop/coins/me", token=token)["coins"]


class SMTPServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True


run(["systemctl", "stop", "postfix.service"])
smtp = SMTPServer(("127.0.0.1", 25), SMTP)
smtp.daemon_threads = True
threading.Thread(target=smtp.serve_forever, daemon=True).start()
try:
    restart_backend()  # discard connections to the stopped ordinary SMTP service
    # A: SMTP success occurs after expiry but the transaction timestamp predates it.
    first, token = user("reviewfirst")
    before = balance(token)
    expiry = sql(
        f"update premium set until=clock_timestamp()+interval '2 seconds' where user_id='{first}' returning until"
    )
    SMTP.delay = 3
    agreement = enable(token)
    SMTP.delay = 0
    actual_sent = SMTP.accepted[-1].isoformat()
    stored_sent = sql(f"select sent_at from premium_renewal_delivery where agreement_id='{agreement}'")
    status = request("GET", "/shop/premium/me", token=token)
    after = balance(token)
    print(
        json.dumps(
            {
                "case": "A-delayed-SMTP",
                "paid_until": expiry,
                "actual_SMTP_acceptance": actual_sent,
                "stored_sent_at": stored_sent,
                "balance_before": before,
                "balance_after": after,
                "status": status,
            },
            indent=2,
        ),
        flush=True,
    )
    assert before == after and not status["premium"], "Late SMTP must never authorize a debit"
    assert (
        sql(
            f"select sent_at >= confirmation_deadline from premium_renewal_delivery d join premium_renewal_agreements a on a.id=d.agreement_id where a.id='{agreement}'"
        )
        == "t"
    )
    request("PUT", "/shop/premium/autopay", {"plan": None}, token)

    # B: a manual purchase after missed confirmation moves max(until), reviving
    # the old agreement without a fresh renewal consent. A second user's enable
    # runs the existing delivery helper without reading/renewing the first user.
    second, token2 = user("reviewsecond")
    third, token3 = user("reviewthird")
    expiry2 = sql(
        f"update premium set until=clock_timestamp()+interval '1 second' where user_id='{second}' returning until"
    )
    SMTP.reject = True
    agreement2 = enable(token2)
    time.sleep(1.1)
    SMTP.reject = False
    enable(token3)
    original_gate = sql(
        f"select d.sent_at < (select max(until) from premium where user_id='{second}') from premium_renewal_delivery d where agreement_id='{agreement2}'"
    )
    before2 = balance(token2)
    manual_status = purchase(token2)
    after_manual = balance(token2)
    sql(
        f"update premium set until=clock_timestamp()+interval '0.2 seconds' where user_id='{second}' and until>clock_timestamp()"
    )
    time.sleep(0.3)
    renewed_status = request("GET", "/shop/premium/me", token=token2)
    after_renewal = balance(token2)
    print(
        json.dumps(
            {
                "case": "B-manual-purchase-revives-late-agreement",
                "original_paid_until": expiry2,
                "confirmation_was_timely": original_gate,
                "agreement_id": agreement2,
                "manual_purchase_response": manual_status,
                "next_expiry_response": renewed_status,
                "balance_before_manual": before2,
                "balance_after_manual": after_manual,
                "balance_after_renewal": after_renewal,
            },
            indent=2,
        ),
        flush=True,
    )
    assert original_gate == "f"
    assert manual_status["autopay"] is None and manual_status["renewal"] is None
    assert before2 - after_manual == 1000 and after_manual == after_renewal
    assert not renewed_status["premium"]
    assert sql(f"select count(*) from premium_renewal_cancellations where agreement_id='{agreement2}'") == "1"

    # C: purchase first, then a successful outbox retry, with no status/task in between.
    fourth, token4 = user("fixpurchasefirst")
    sql(f"update premium set until=clock_timestamp()+interval '1 second' where user_id='{fourth}'")
    SMTP.reject = True
    agreement4 = enable(token4)
    time.sleep(1.1)
    before4 = balance(token4)
    manual4 = purchase(token4, observe_status=False)
    assert manual4["state"] == "fulfilled"
    assert sql(f"select count(*) from premium_subscriptions where user_id='{fourth}'") == "0"
    SMTP.reject = False
    enable(token3)  # actual retry after the ordinary purchase
    assert sql(f"select sent_at is not null from premium_renewal_delivery where agreement_id='{agreement4}'") == "t"
    assert sql(f"select count(*) from premium_subscriptions where user_id='{fourth}'") == "0"
    sql(
        f"update premium set until=clock_timestamp()+interval '0.1 second' where user_id='{fourth}' and until>clock_timestamp()"
    )
    time.sleep(0.2)
    assert not request("GET", "/shop/premium/me", token=token4)["premium"]
    assert balance(token4) == before4 - 1000
    print("PASS C: purchase before late delivery stays OFF; only intentional purchase debited", flush=True)

    # Replay the ended request cannot replace it. Fresh explicit consent can.
    purchase(token4)
    enable(token4, agreement4)
    assert request("GET", "/shop/premium/me", token=token4)["autopay"] is None
    fresh4 = enable(token4)
    status4 = request("GET", "/shop/premium/me", token=token4)
    assert fresh4 != agreement4 and status4["renewal"]["id"] == fresh4
    assert status4["renewal"]["confirmation_sent"]
    request("PUT", "/shop/premium/autopay", {"plan": None}, token4)
    enable(token4, fresh4)
    assert request("GET", "/shop/premium/me", token=token4)["autopay"] is None
    print("PASS replay: ended IDs stay OFF; new explicit request creates a new agreement", flush=True)

    # D: even an early manual extension must not rewrite the confirmation deadline.
    fifth, token5 = user("fixearlyextension")
    deadline5 = sql(
        f"update premium set until=clock_timestamp()+interval '2 seconds' where user_id='{fifth}' returning until"
    )
    SMTP.reject = True
    agreement5 = enable(token5)
    before5 = balance(token5)
    manual5 = purchase(token5)
    assert manual5["renewal"]["id"] == agreement5 and not manual5["renewal"]["confirmation_sent"]
    assert (
        sql(
            f"select confirmation_deadline = '{deadline5}'::timestamptz from premium_renewal_agreements where id='{agreement5}'"
        )
        == "t"
    )
    time.sleep(2.1)
    SMTP.reject = False
    enable(token3)
    status5 = request("GET", "/shop/premium/me", token=token5)
    assert status5["premium"] and status5["autopay"] is None
    assert balance(token5) == before5 - 1000
    print(
        "PASS D: pre-expiry manual extension preserves paid access but never erases missed confirmation deadline",
        flush=True,
    )

    # E: the later item inherits no eligibility from a batch's earlier start.
    batchfirst, batchtoken1 = user("fixbatchfirst")
    batchlater, batchtoken2 = user("fixbatchlater")
    SMTP.reject = True
    enable(batchtoken1)
    deadline_batch = sql(
        f"update premium set until=clock_timestamp()+interval '2 seconds' where user_id='{batchlater}' returning until"
    )
    batch_agreement = enable(batchtoken2)
    before_batch = balance(batchtoken2)
    SMTP.reject = False
    SMTP.delay = 3
    enable(token3)
    SMTP.delay = 0
    timing = sql(
        f"select sent_at >= confirmation_deadline from premium_renewal_delivery d join premium_renewal_agreements a on a.id=d.agreement_id where a.id='{batch_agreement}'"
    )
    assert timing == "t"
    status_batch = request("GET", "/shop/premium/me", token=batchtoken2)
    assert not status_batch["premium"] and balance(batchtoken2) == before_batch
    print(
        json.dumps(
            {
                "case": "E-later-batch-item",
                "deadline": deadline_batch,
                "late_recorded": timing,
                "coins_unchanged": balance(batchtoken2),
            },
            indent=2,
        ),
        flush=True,
    )

    # F: cancellation during SMTP + replay of that request remain terminal.
    concurrent, concurrent_token = user("fixconcurrent")
    concurrent_id = str(uuid.uuid4())
    SMTP.started.clear()
    SMTP.delay = 2
    errors = []

    def enable_concurrently():
        try:
            enable(concurrent_token, concurrent_id)
        except Exception as exc:
            errors.append(exc)

    enabling = threading.Thread(target=enable_concurrently)
    enabling.start()
    assert SMTP.started.wait(5)
    request("PUT", "/shop/premium/autopay", {"plan": None}, concurrent_token)
    enable(concurrent_token, concurrent_id)
    enabling.join(10)
    SMTP.delay = 0
    assert not enabling.is_alive() and not errors, errors
    assert request("GET", "/shop/premium/me", token=concurrent_token)["autopay"] is None
    assert sql(f"select sent_at is not null from premium_renewal_delivery where agreement_id='{concurrent_id}'") == "t"
    print("PASS F: cancellation and replay while SMTP is in flight cannot reactivate renewal", flush=True)

    # G: a timely confirmed agreement survives ordinary extension and keeps its
    # agreed price even after the current offered/manual price changes.
    valid, valid_token = user("fixvalid")
    valid_id = enable(valid_token)
    original_deadline = sql(f"select confirmation_deadline from premium_renewal_agreements where id='{valid_id}'")
    isolated_status = request("GET", "/shop/premium/me", token=token3)
    isolated_balance = balance(token3)
    CONFIG.write_bytes(ORIGINAL_CONFIG + b"\n[premium]\nmonthly_price = 1500\n")
    restart_backend()
    assert request("GET", "/shop/premium/renewal-offer")["monthly_price"] == 1500
    before_valid = balance(valid_token)
    extension = purchase(valid_token)
    after_valid_manual = balance(valid_token)
    assert extension["renewal"]["id"] == valid_id and extension["renewal"]["confirmation_sent"]
    assert before_valid - after_valid_manual == 1500
    assert (
        sql(f"select confirmation_deadline from premium_renewal_agreements where id='{valid_id}'") == original_deadline
    )
    sql(f"update premium set until=clock_timestamp()+interval '0.1 second' where user_id='{valid}'")
    time.sleep(0.2)
    valid_renewed = request("GET", "/shop/premium/me", token=valid_token)
    assert valid_renewed["premium"] and valid_renewed["renewal"]["id"] == valid_id
    assert valid_renewed["renewal"]["monthly_price"] == 1000
    assert after_valid_manual - balance(valid_token) == 1000
    assert request("GET", "/shop/premium/me", token=token3) == isolated_status
    assert balance(token3) == isolated_balance
    print(
        json.dumps(
            {
                "case": "G-timely-confirmed-extension",
                "manual_price": before_valid - after_valid_manual,
                "agreed_renewal_debit": after_valid_manual - balance(valid_token),
                "same_agreement": valid_id,
                "unrelated_user_unchanged": True,
            },
            indent=2,
        ),
        flush=True,
    )

    # Repository and migration tests are separate CI jobs; this VM owns the
    # actual HTTP/SMTP deadline, extension, replay and isolation assertions.
    print("PASS all actual HTTP/SMTP deadline, extension, replay and isolation regressions", flush=True)

finally:
    CONFIG.write_bytes(ORIGINAL_CONFIG)
    run(["systemctl", "restart", "academy-backend.service"])
    smtp.shutdown()
    smtp.server_close()
    run(["systemctl", "start", "postfix.service"])
