"""Actual HTTP/SMTP contract attachments in an owned PostgreSQL 18 fixture.

All accounts, balances and historical rows are synthetic. Provider endpoints
stay on loopback; SMTP accepts only example.invalid recipients.
"""

import argparse
from email import policy
from email.parser import BytesParser
import hashlib
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
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen
from uuid import uuid4

from purchase_provider import provider_handler

ROOT = Path(__file__).resolve().parents[1]
FOO = "a8d95e0f-71ae-4c49-995e-695b7c93848c"


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def quote(value):
    return "'" + str(value).replace("'", "''") + "'"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/academy")
    parser.add_argument("--pg-bin", type=Path, required=True)
    parser.add_argument("--valkey", type=Path, required=True)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    args = parser.parse_args()
    assert os.geteuid() != 0, "fixture must run as an unprivileged owner"
    assert "(PostgreSQL) 18." in subprocess.check_output([args.pg_bin / "postgres", "--version"], text=True)
    evidence = args.evidence_dir.resolve()
    evidence.mkdir(mode=0o700, parents=True, exist_ok=False)
    base = Path(tempfile.mkdtemp(prefix="academy-purchase-artifacts-", dir="/tmp")).resolve()
    owner = base / "OWNER.json"
    marker = json.dumps({"uid": os.geteuid(), "run": str(uuid4()), "root": str(base)})
    owner.write_text(marker)
    pg_port, cache_port, api_port = free_port(), free_port(), free_port()
    url = f"postgres://academy_purchase_tests@127.0.0.1:{pg_port}/postgres"
    messages = []
    orders = {}
    provider = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), provider_handler(orders, [], {"render_fail": False}, threading.Event(), threading.Event())
    )
    provider.daemon_threads = True
    threading.Thread(target=provider.serve_forever, daemon=True).start()

    class Sink(socketserver.StreamRequestHandler):
        def handle(self):
            self.wfile.write(b"220 owned SMTP fixture\r\n")
            while line := self.rfile.readline():
                verb = line.split(b" ", 1)[0].strip().upper()
                if verb in [b"EHLO", b"HELO"]:
                    self.wfile.write(b"250-localhost\r\n250 8BITMIME\r\n")
                elif verb == b"RCPT":
                    assert line.rstrip().endswith(b"@example.invalid>"), "non-fixture recipient rejected"
                    self.wfile.write(b"250 OK\r\n")
                elif verb == b"DATA":
                    self.wfile.write(b"354 data\r\n")
                    raw = []
                    while (line := self.rfile.readline()) not in [b".\r\n", b""]:
                        raw.append(line[1:] if line.startswith(b"..") else line)
                    message = BytesParser(policy=policy.default).parsebytes(b"".join(raw))
                    messages.append(message)
                    self.wfile.write(b"250 accepted\r\n")
                elif verb == b"QUIT":
                    self.wfile.write(b"221 bye\r\n")
                    return
                else:
                    self.wfile.write(b"250 OK\r\n")

    class SMTP(socketserver.ThreadingTCPServer):
        daemon_threads = True

    smtp = SMTP(("127.0.0.1", 0), Sink)
    threading.Thread(target=smtp.serve_forever, daemon=True).start()
    config = (ROOT / "config.dev.toml").read_text()
    config = config.replace("postgres://academy@127.0.0.1:5432/academy", url)
    config = config.replace("127.0.0.1:6379", f"127.0.0.1:{cache_port}")
    config = config.replace("127.0.0.1:8000", f"127.0.0.1:{api_port}")
    config = config.replace("smtp://academy:academy@127.0.0.1:2525", f"smtp://127.0.0.1:{smtp.server_address[1]}")
    config = config.replace("http://127.0.0.1:8103/", f"http://127.0.0.1:{provider.server_address[1]}/")
    config = config.replace("http://127.0.0.1:8001/", f"http://127.0.0.1:{provider.server_address[1]}/")
    for archive in [".invoices", ".credit_notes", ".final_statements"]:
        config = config.replace('"' + archive + '"', '"' + str(base / archive) + '"')
    config += "\n[purchase.provision_window_seconds]\npremium_monthly=86400\npremium_yearly=86400\nhearts=86400\ncourse=86400\ncoins=86400\n"
    fixture = base / "config.toml"
    fixture.write_text(config)
    fixture.chmod(0o600)
    env = {
        k: v for k, v in os.environ.items() if not k.startswith("PG") and k not in ["ACADEMY_CONFIG", "DATABASE_URL"]
    }
    env.update(ACADEMY_CONFIG=str(fixture), RUST_LOG="warn")
    children = []
    pg_started = False
    result = {"passed": False, "checks": [], "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest()}

    def run(*argv):
        with (evidence / "fixture.log").open("ab") as log:
            subprocess.run(argv, cwd=ROOT, env=env, check=True, stdout=log, stderr=log)

    def sql(command):
        response = subprocess.run(
            [args.pg_bin / "psql", url, "-XqAt", "-v", "ON_ERROR_STOP=1"],
            input=command,
            env=env,
            text=True,
            capture_output=True,
        )
        assert response.returncode == 0, response.stderr
        return response.stdout.strip()

    def request(path, method="GET", body=None, token=None):
        headers = {"Content-Type": "application/json"}
        if token:
            headers["Authorization"] = "Bearer " + token
        req = Request(
            f"http://127.0.0.1:{api_port}" + path,
            method=method,
            headers=headers,
            data=json.dumps(body).encode() if body is not None else None,
        )
        try:
            with urlopen(req, timeout=20) as response:
                return response.status, response.read()
        except HTTPError as error:
            return error.code, error.read()

    def check_mail(message, expected, names):
        parts = list(message.iter_attachments())
        assert [p.get_filename() for p in parts] == names
        assert [p.get_content_type() for p in parts] == ["application/pdf"] * len(names)
        assert [p.get_payload(decode=True) for p in parts] == expected
        return [
            {"filename": name, "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
            for name, data in zip(names, expected)
        ]

    def start():
        log = (evidence / "backend.log").open("ab")
        process = subprocess.Popen([args.binary, "serve"], cwd=ROOT, env=env, stdout=log, stderr=log)
        log.close()
        children.append(process)
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            assert process.poll() is None, "backend startup failed; inspect backend.log"
            try:
                if request("/auth/session")[0] == 401:
                    return process
            except (URLError, ConnectionError):
                pass
            time.sleep(0.1)
        raise AssertionError("owned backend did not become ready")

    try:
        run(
            args.pg_bin / "initdb",
            "-D",
            base / "pgdata",
            "-U",
            "academy_purchase_tests",
            "--auth=trust",
            "--no-locale",
            "--encoding=UTF8",
        )
        with (base / "pgdata/postgresql.conf").open("a") as conf:
            conf.write(f"\nlisten_addresses='127.0.0.1'\nport={pg_port}\nunix_socket_directories='{base}'\n")
        run(args.pg_bin / "pg_ctl", "-D", base / "pgdata", "-l", base / "postgres.log", "-w", "start")
        pg_started = True
        assert Path(sql("SHOW data_directory")).resolve() == base / "pgdata"
        cache = subprocess.Popen(
            [
                args.valkey,
                "--bind",
                "127.0.0.1",
                "--port",
                str(cache_port),
                "--save",
                "",
                "--appendonly",
                "no",
                "--dir",
                base,
                "--logfile",
                base / "valkey.log",
            ],
            env=env,
        )
        children.append(cache)
        run(args.binary, "migrate", "demo", "--force")
        sql(
            f"UPDATE users SET email='purchase-test@example.invalid',email_verified=true,terms_version='2026-09-r1' WHERE id='{FOO}'; INSERT INTO coins(user_id,coins,withheld_coins) VALUES('{FOO}',100000,0) ON CONFLICT(user_id) DO UPDATE SET coins=100000; DELETE FROM premium_subscriptions WHERE user_id='{FOO}'; DELETE FROM premium WHERE user_id='{FOO}'"
        )
        server = start()
        status, data = request("/auth/sessions", "POST", {"name_or_email": "foo", "password": "foo password"})
        assert status == 200, data
        session = json.loads(data)
        token = session["access_token"]
        current_terms = (ROOT / "academy_assets/assets/email/agb-2026-09-r4.pdf").read_bytes()
        current_withdrawal = (ROOT / "academy_assets/assets/email/widerrufsbelehrung-2026-09-r1.pdf").read_bytes()
        status, data = request("/shop/coins/paypal/offers/1337", "POST", token=token)
        assert status == 200, data
        coin_offer = json.loads(data)["offer"]
        status, data = request(
            "/shop/coins/paypal/orders",
            "POST",
            {
                "coins": 1337,
                "order_id": coin_offer["id"],
                "offer_hash": coin_offer["hash"],
                "accepted": True,
                "early_performance_requested": True,
            },
            token,
        )
        assert status == 200, data
        paypal_id = json.loads(data)
        orders[paypal_id]["remote"]["status"] = "APPROVED"
        count = len(messages)
        status, data = request(f"/shop/coins/paypal/orders/{paypal_id}/capture", "POST", token=token)
        assert status == 200 and len(messages) == count + 2, data
        contract_mail = next(message for message in messages[count:] if "purchase-" in str(message["Message-ID"]))
        invoice_mail = next(message for message in messages[count:] if "invoice-" in str(message["Message-ID"]))
        result["checks"].append(
            {
                "new_purchase": "coins (loopback provider)",
                "attachments": check_mail(
                    contract_mail,
                    [current_terms, current_withdrawal],
                    ["vereinbarte-agb.pdf", "vereinbarte-widerrufsinformation.pdf"],
                ),
            }
        )
        invoice = json.loads(sql(f"SELECT artifact FROM paypal_receipt_artifacts WHERE order_id={quote(paypal_id)}"))
        result["checks"].append(
            {
                "coin_invoice": "stored receipt",
                "attachments": check_mail(
                    invoice_mail,
                    [bytes(part["bytes"]) for part in invoice["attachments"]],
                    [part["filename"] for part in invoice["attachments"]],
                ),
            }
        )
        assert bytes(invoice["attachments"][1]["bytes"]) == current_terms
        assert bytes(invoice["attachments"][2]["bytes"]) == current_withdrawal
        internal = subprocess.check_output(
            [args.binary, "jwt", "sign", json.dumps({"aud": "shop"})], env=env, text=True
        ).strip()
        status, data = request(
            f"/shop/_internal/purchase-offers/skills/{FOO}",
            "POST",
            {
                "kind": "course",
                "reference": "synthetic-attachment-course",
                "title": "Synthetic course",
                "description": "Local attachment test",
                "coins": 1000,
                "facts": {},
                "revision": "fixture",
                "service_starts_at": None,
            },
            internal,
        )
        assert status == 200, data
        course_offer = json.loads(data)["offer"]
        count = len(messages)
        status, data = request(
            f"/shop/_internal/purchases/skills/{FOO}",
            "POST",
            {
                "order_id": course_offer["id"],
                "offer_hash": course_offer["hash"],
                "accepted": True,
                "early_performance_requested": True,
            },
            internal,
        )
        assert status == 200 and len(messages) == count + 1, data
        result["checks"].append(
            {
                "new_purchase": "course (internal fixture)",
                "attachments": check_mail(
                    messages[-1],
                    [current_terms, current_withdrawal],
                    ["vereinbarte-agb.pdf", "vereinbarte-widerrufsinformation.pdf"],
                ),
            }
        )
        for kind in ["premium_monthly", "premium_yearly", "hearts"]:
            if kind == "hearts":
                sql(
                    f"INSERT INTO hearts(user_id,hearts,last_refill) VALUES('{FOO}',1,clock_timestamp()) ON CONFLICT(user_id) DO UPDATE SET hearts=1,last_refill=clock_timestamp()"
                )
            status, data = request("/shop/purchases/offers/" + kind, "POST", token=token)
            assert status == 200, data
            offer = json.loads(data)["offer"]
            for document, expected in [("terms", current_terms), ("withdrawal", current_withdrawal)]:
                status, data = request(f"/shop/purchases/{offer['id']}/documents/{document}", token=token)
                assert status == 200 and data == expected
            count = len(messages)
            status, data = request(
                "/shop/purchases/accept",
                "POST",
                {
                    "order_id": offer["id"],
                    "offer_hash": offer["hash"],
                    "accepted": True,
                    "early_performance_requested": True,
                },
                token,
            )
            assert status == 200 and json.loads(data)["state"] == "fulfilled", data
            assert len(messages) == count + 1
            attachments = check_mail(
                messages[-1],
                [current_terms, current_withdrawal],
                ["vereinbarte-agb.pdf", "vereinbarte-widerrufsinformation.pdf"],
            )
            assert sql(f"SELECT terms_version FROM users WHERE id='{FOO}'") == "2026-09-r1"
            result["checks"].append({"new_purchase": kind, "account_terms_unchanged": True, "attachments": attachments})
            if kind == "hearts":
                fulfillment = json.loads(data)["fulfillment"]
                assert fulfillment["added"] == 5 and fulfillment["hearts_after"] == 6
        status, data = request("/shop/premium/renewal-offer", token=token)
        assert status == 200, data
        renewal = json.loads(data)
        assert renewal["terms_version"] == "2026-09-r4"
        count = len(messages)
        status, data = request(
            "/shop/premium/autopay",
            "PUT",
            {
                "plan": "MONTHLY",
                "consent": {
                    "request_id": str(uuid4()),
                    "offer_id": renewal["id"],
                    "accepted": True,
                    "withdrawal_consent": True,
                },
            },
            token,
        )
        assert status == 200 and len(messages) == count + 1, data
        result["checks"].append(
            {
                "new_renewal": "r4 on r1 account",
                "attachments": check_mail(
                    messages[-1],
                    [current_terms, current_withdrawal],
                    ["vereinbarte-agb.pdf", "vereinbarte-widerrufsbelehrung.pdf"],
                ),
            }
        )
        server.terminate()
        server.wait(timeout=15)
        expected_messages = []
        for manifest_path in sorted((ROOT / "academy_assets/assets/email").glob("purchase-document-manifest*.json")):
            manifest = json.loads(manifest_path.read_text())
            docs = [ROOT / Path(doc["pdf"]).relative_to("backend") for doc in manifest["documents"]]
            data = [doc.read_bytes() for doc in docs]
            oid = str(uuid4())
            historical = dict(offer, id=oid, source="skills")
            historical["product"] = dict(offer["product"], kind="course", reference="historical-synthetic", coins=0)
            metadata = {
                "sender": "Bootstrap Academy DEV <dev@bootstrap.academy>",
                "message_id": f"matrix-{oid}@example.invalid",
                "subject": f"Stored {manifest['release']}",
                "content_type": "text/plain; charset=utf-8",
                "attachment_content_type": "application/pdf",
                "terms_filename": docs[0].name,
                "withdrawal_filename": docs[1].name,
            }
            sql(
                f"INSERT INTO purchase_offers(id,user_id,source,offer,terms_pdf,withdrawal_pdf,created_at,expires_at) VALUES('{oid}','{FOO}','skills',{quote(json.dumps(historical))}::jsonb,decode('{data[0].hex()}','hex'),decode('{data[1].hex()}','hex'),clock_timestamp(),clock_timestamp()+interval '1 day'); INSERT INTO purchase_acceptances(order_id,confirmation_body,message_metadata) VALUES('{oid}','Synthetic original contract',{quote(json.dumps(metadata))}::jsonb); INSERT INTO purchase_progress(order_id,state) VALUES('{oid}','review')"
            )
            expected_messages.append((metadata["subject"], data, [doc.name for doc in docs]))
            agreement = str(uuid4())
            body = f"Synthetic stored renewal {manifest['release']}"
            sql(
                f"INSERT INTO premium_renewal_agreements(id,user_id,received_at,offer_id,monthly_price,recipient,document,terms_pdf,withdrawal_pdf,paid_period_id,confirmation_deadline) VALUES('{agreement}','{FOO}',clock_timestamp(),'synthetic',777,'purchase-test@example.invalid',{quote(body)},decode('{data[0].hex()}','hex'),decode('{data[1].hex()}','hex'),(SELECT id FROM premium WHERE user_id='{FOO}' ORDER BY until DESC LIMIT 1),clock_timestamp()+interval '1 day'); INSERT INTO premium_renewal_delivery(agreement_id) VALUES('{agreement}')"
            )
        count = len(messages)
        server = start()
        deadline = time.monotonic() + 15
        while len(messages) < count + len(expected_messages) and time.monotonic() < deadline:
            time.sleep(0.1)
        assert len(messages) == count + len(expected_messages)
        for subject, data, names in expected_messages:
            matching = [message for message in messages[count:] if str(message["Subject"]) == subject]
            assert len(matching) == 1
            result["checks"].append({"historical_replay": subject, "attachments": check_mail(matching[0], data, names)})
        count = len(messages)
        run(args.binary, "task", "refresh-premium")
        assert len(messages) == count + len(expected_messages)
        for subject, data, _ in expected_messages:
            release = subject.removeprefix("Stored ")
            matching = [
                message
                for message in messages[count:]
                if f"Synthetic stored renewal {release}"
                == message.get_body(preferencelist=("plain",)).get_content().strip()
            ]
            assert len(matching) == 1
            result["checks"].append(
                {
                    "historical_renewal": release,
                    "attachments": check_mail(
                        matching[0], data, ["vereinbarte-agb.pdf", "vereinbarte-widerrufsbelehrung.pdf"]
                    ),
                }
            )
        server.terminate()
        server.wait(timeout=15)
        # A future version without its approved bundle fails before any mail or API starts.
        unknown = base / "unknown.toml"
        unknown.write_text(
            config
            + "\n[learning_policy]\nmode='daily'\nterms_version='2099-unknown'\naccepted_since='2026-10-03T00:00:00Z'\nregistered_since='2026-10-03T00:00:00Z'\n"
        )
        failed = subprocess.run(
            [args.binary, "serve"],
            env=env | {"ACADEMY_CONFIG": str(unknown)},
            cwd=ROOT,
            capture_output=True,
            timeout=20,
        )
        error = failed.stdout + failed.stderr
        (evidence / "unknown-version.log").write_bytes(error)
        assert failed.returncode != 0 and b"approved purchase document bundle" in error, error
        assert len(messages) == count + len(expected_messages)
        result["checks"].append({"unknown_unapproved_version": "startup rejected, no SMTP delivery"})
        result["smtp_messages"] = len(messages)
        result["passed"] = True
        print(f"PASS {len(result['checks'])} attachment checks, {len(messages)} isolated SMTP messages", flush=True)
    finally:
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
            child.wait(timeout=15)
        smtp.shutdown()
        smtp.server_close()
        provider.shutdown()
        provider.server_close()
        if pg_started:
            run(args.pg_bin / "pg_ctl", "-D", base / "pgdata", "-m", "fast", "-w", "stop")
        assert owner.read_text() == marker and base.stat().st_uid == os.geteuid()
        shutil.rmtree(base)
        result["fixture_removed"] = not base.exists()
        (evidence / "result.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
