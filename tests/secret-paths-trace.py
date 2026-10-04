"""Compare delivered codes, OAuth, CAPTCHA and finance bearers to actual TRACE.

Backend logs, SMTP bodies and credentials stay in memory. Evidence contains
boolean comparisons and hashes. It also verifies the password/hash/session
redaction inherited from the merged PR774.
"""

import argparse
import base64
from datetime import date, timedelta
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import threading
import urllib.parse
import urllib.error
import urllib.request

REPO = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("secret_trace_fixture", REPO / "tests/backend-safety.py")
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)


class TraceFixture(fixture.Fixture):
    def __init__(self, args):
        super().__init__(args)
        self.env["RUST_LOG"] = "trace"
        self.captures = []
        self.tokens = set()

    def spawn(self, name, args):
        if name != "backend":
            return super().spawn(name, args)
        process = subprocess.Popen(
            [str(arg) for arg in args], cwd=REPO, env=self.env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT
        )
        self.processes.append(process)
        chunks = bytearray()

        def drain():
            while chunk := os.read(process.stdout.fileno(), 65536):
                chunks.extend(chunk)
            process.stdout.close()

        thread = threading.Thread(target=drain)
        thread.start()
        self.captures.append((chunks, thread))
        return process

    def request(self, *args, **kwargs):
        value = super().request(*args, **kwargs)
        body = value[1] if isinstance(value, tuple) else value
        if isinstance(body, dict):
            for key in ("access_token", "refresh_token"):
                if key in body:
                    self.tokens.add(body[key])
        return value

    def close(self):
        try:
            super().close()
        finally:
            for _, thread in self.captures:
                thread.join(timeout=15)
                assert not thread.is_alive(), "owned backend TRACE reader did not stop"


class Mail(fixture.SMTP):
    def handle(self):
        self.wfile.write(b"220 isolated test SMTP\r\n")
        while line := self.rfile.readline():
            verb = line.split(b" ", 1)[0].strip().upper()
            if verb in (b"EHLO", b"HELO"):
                self.wfile.write(b"250-localhost\r\n250 8BITMIME\r\n")
            elif verb == b"DATA":
                self.wfile.write(b"354 send data\r\n")
                chunks = []
                while (line := self.rfile.readline()) not in (b".\r\n", b""):
                    chunks.append(line)
                self.server.bodies.append(b"".join(chunks).decode(errors="replace"))
                self.server.messages += 1
                self.wfile.write(b"250 accepted\r\n")
            elif verb == b"QUIT":
                self.wfile.write(b"221 bye\r\n")
                return
            else:
                self.wfile.write(b"250 OK\r\n")


class Captcha(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        data = urllib.parse.parse_qs(self.rfile.read(int(self.headers["Content-Length"])).decode())
        self.server.calls.append(data)
        response = b'{"success":true,"score":0.9}'
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)


class Peer(fixture.Peer):
    def do_GET(self):
        authorization = self.headers.get("Authorization", "")
        if authorization.startswith("Bearer "):
            self.server.tokens.add(authorization[7:])
        super().do_GET()


class Fixture(TraceFixture):
    def __init__(self, args):
        super().__init__(args)
        self.captcha_enabled = False
        self.captcha_secret = "owned-synthetic-captcha-server-secret"
        self.oauth_secret = "owned-synthetic-oauth-client-secret"

    def server(self, server):
        if isinstance(server, fixture.SMTPServer):
            server.RequestHandlerClass = Mail
            server.bodies = []
        elif isinstance(server, http.server.ThreadingHTTPServer) and server.RequestHandlerClass == fixture.Peer:
            server.RequestHandlerClass = Peer
            server.tokens = set()
        return super().server(server)

    def write_config(self):
        super().write_config()
        if not hasattr(self, "captcha_server"):
            self.captcha_server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Captcha)
            self.captcha_server.calls = []
            self.server(self.captcha_server)
        port = self.captcha_server.server_address[1]
        with (self.base / "fixture.toml").open("a") as f:
            f.write(
                f"""\n[recaptcha]
enable = {str(self.captcha_enabled).lower()}
sitekey = "owned-public-sitekey"
secret = "{self.captcha_secret}"
siteverify_endpoint_override = "http://127.0.0.1:{port}/siteverify"
[oauth2]
enable = true
redirect_uris = ["http://127.0.0.1:9/callback"]
[oauth2.providers.review]
enable = true
pkce = true
name = "Review"
client_id = "owned-public-client-id"
client_secret = "{self.oauth_secret}"
auth_url = "http://127.0.0.1:9/authorize"
token_url = "http://127.0.0.1:9/token"
userinfo_url = "http://127.0.0.1:9/userinfo"
userinfo_id_key = "id"
userinfo_name_key = "name"
scopes = []
"""
            )

    def last_mail_code(self):
        values = re.findall(r"\b[A-Z0-9]{4}(?:-[A-Z0-9]{4}){3}\b", self.smtp.bodies[-1])
        assert values, "SMTP fixture did not receive an actual verification code"
        return values[0]


def check_finance_downloads(f, user, foreign, access_token):
    """Issue real bearers and use them for owned archived invoice/credit PDFs."""
    invoice = b"%PDF-1.4\n% isolated invoice fixture\n%%EOF\n"
    credit = b"%PDF-1.4\n% isolated credit-note fixture\n%%EOF\n"
    folder = f.base / "invoices"
    folder.mkdir(exist_ok=True)
    (folder / "R0000042.pdf").write_bytes(invoice)
    f.sql(
        f"INSERT INTO financial_documents(number,kind,user_id,issued_at) "
        f"VALUES('R0000042','invoice','{user['id']}',now())"
    )
    number = f.sql(
        f"INSERT INTO user_numbers(user_id,number) VALUES('{user['id']}',nextval('user_number')) RETURNING number"
    )
    month = date.today().replace(day=1) - timedelta(days=1)
    folder = f.base / "credits"
    folder.mkdir(exist_ok=True)
    (folder / f"G{month.year:04}{month.month:02}-{number}.pdf").write_bytes(credit)
    token = f.request("/finance/token", token=access_token)
    assert isinstance(token, str) and len(token.split(".")) == 3, "finance bearer not issued"
    foreign_token = f.request("/finance/token", token=f.token(foreign))
    assert foreign_token != token, "separate recipients must have separate finance authority"
    invalid = "owned-synthetic-invalid-finance-bearer"

    def download(path, expected, pdf=None, *, method="GET", access=None):
        headers = {} if access is None else {"Authorization": f"Bearer {access}"}
        request = urllib.request.Request(f"http://127.0.0.1:{f.ports['api']}{path}", method=method, headers=headers)
        if access is None:
            assert "Authorization" not in request.headers
        try:
            response = urllib.request.urlopen(request, timeout=15)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            status, body = response.status, response.read()
        assert status == expected, f"finance download expected {expected}, got {status}; URI omitted"
        if pdf is not None:
            assert body == pdf, "finance download did not return the exact owned original"

    invoice_path = f"/finance/invoices/{token}/42/invoice.pdf"
    credit_path = f"/finance/credit_notes/{token}/{month.year}/{month.month}/credit_note.pdf"
    download(invoice_path, 200, invoice)
    download(credit_path, 200, credit)
    encoded = token.replace(".", "%2E")
    download(f"/finance/invoices/{encoded}/42/invoice.pdf", 200, invoice)
    download(f"/finance/invoices/{invalid}/42/invoice.pdf", 401)
    download(f"/finance/credit_notes/{invalid}/{month.year}/{month.month}/credit_note.pdf", 401)
    download(f"/finance/invoices/{foreign_token}/42/invoice.pdf", 404)
    download(f"/finance/invoices/{f.auth}/42/invoice.pdf", 401)
    # Query values, parse failures and unmatched routes must also stay out of
    # request spans. All successful downloads use only the path bearer.
    download(invoice_path + f"?secret={token}", 200, invoice)
    download(f"/finance/invoices/{token}/not-a-number/invoice.pdf", 400)
    download(invoice_path + "/unmatched", 404)
    # The audit service is instrumented before its admin check, so also cover
    # regular access tokens. Real admin entries must retain useful metadata and
    # redact the bearer before both persistence and TRACE.
    download(invoice_path, 405, method="POST", access=access_token)
    f.sql(f"UPDATE users SET admin=true WHERE id='{user['id']}'")
    download(invoice_path, 405, method="POST", access=access_token)
    download(credit_path, 405, method="POST", access=access_token)
    download(invoice_path + "/unmatched", 404, method="POST", access=access_token)
    download(f"/finance/invoices/{encoded}/42/invoice.pdf", 405, method="POST", access=access_token)
    entries = json.loads(
        f.sql(
            "SELECT json_agg(json_build_object('method',method,'path',path,'status',status,'request_id',request_id)) "
            f"FROM admin_audit_log WHERE admin_user_id='{user['id']}' AND path LIKE '/finance/%'"
        )
    )
    assert len(entries) == 4, "rejected admin finance requests must retain audit entries"
    assert sorted(entry["status"] for entry in entries) == [404, 405, 405, 405]
    assert all(entry["method"] == "POST" and entry["request_id"] for entry in entries), "audit metadata lost"
    tokens = {
        "issued_finance_bearer": token,
        "issued_foreign_finance_bearer": foreign_token,
        "encoded_finance_bearer": encoded,
        "invalid_finance_bearer": invalid,
    }
    audit = {
        "entries_compared": len(entries),
        "paths_redacted": all("<redacted>" in entry["path"] for entry in entries),
        "bearer_matches": any(value in entry["path"] for value in tokens.values() for entry in entries),
        "metadata_preserved": True,
        "rejected_regular_request_status": 405,
    }
    return tokens, audit


parser = argparse.ArgumentParser()
parser.add_argument("--debug-binary", type=Path, required=True)
parser.add_argument("--release-binary", type=Path, required=True)
parser.add_argument("--pg-bin", type=Path, required=True)
parser.add_argument("--valkey", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
os.umask(0o077)
OUT = args.output.resolve()
OUT.mkdir(parents=True, exist_ok=False)

report = {
    "checks": [],
    "credential_secrecy_passed": False,
    "scope": "R774-03..05, R775-01 and internal JWTs",
    "probe_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
}
try:
    for build, binary in [("debug", args.debug_binary), ("release", args.release_binary)]:
        owned = argparse.Namespace(
            binary=binary.resolve(), pg_bin=args.pg_bin.resolve(), valkey=args.valkey.resolve(), output=OUT / build
        )
        with Fixture(owned) as f:
            user = f.account()
            foreign = f.account()
            f.sql(f"UPDATE users SET email_verified=false WHERE id='{user['id']}'")
            f.request("/auth/users/me/email", "POST", token=f.token(user))
            verification = f.last_mail_code()
            f.request("/auth/users/me/email", "PUT", {"code": verification})
            assert f.sql(f"SELECT email_verified FROM users WHERE id='{user['id']}'") == "t"
            url = f.request(
                "/auth/oauth/authorize",
                "POST",
                {"provider_id": "review", "redirect_uri": "http://127.0.0.1:9/callback"},
            )
            state = url["state"]
            challenge = urllib.parse.parse_qs(urllib.parse.urlparse(url["authorize_url"]).query)["code_challenge"][0]
            f.captcha_enabled = True
            f.restart()
            captcha_response = "owned-synthetic-captcha-user-response"
            f.request(
                "/auth/password_reset", "POST", {"email": user["user"]["email"], "recaptcha_response": captcha_response}
            )
            reset_code = f.last_mail_code()
            assert reset_code != verification
            assert f.captcha_server.calls[-1] == {"response": [captcha_response], "secret": [f.captcha_secret]}
            new_password = "owned-synthetic-new-password-for-reset"
            f.request(
                "/auth/password_reset",
                "PUT",
                {"email": user["user"]["email"], "code": reset_code, "password": new_password},
            )
            actual_hash = f.sql(f"SELECT password_hash FROM user_passwords WHERE user_id='{user['id']}'")
            new_login = f.login(user["name"], new_password)
            f.request("/auth/users/me", token=f.auth, expected=401)
            f.request(f"/shop/_internal/learning-policy/{user['id']}", token=f.shop)
            f.request(f"/shop/_internal/learning-policy/{user['id']}", token=f.auth, expected=401)
            f.request("/auth/users/me/export", token=new_login["access_token"])
            finance_tokens, finance_audit = check_finance_downloads(f, user, foreign, new_login["access_token"])
            # Earlier requests may issue another token for the same audience
            # with a different expiry. Require all services and compare every
            # captured token, regardless of the number of distinct issuances.
            audiences = {
                json.loads(base64.urlsafe_b64decode(token.split(".")[1] + "==="))["aud"] for token in f.peer.tokens
            }
            assert audiences == {"skills", "events", "challenges"}, "internal audience coverage incomplete"
            f.backend.terminate()
            f.backend.wait(timeout=15)
            for _, thread in f.captures:
                thread.join(timeout=15)
                assert not thread.is_alive()
            raw = b"\n".join(chunks for chunks, _ in f.captures).decode(errors="replace")
            raw = re.sub(r"\x1b\[[0-9;]*m", "", raw)
            known = {
                **finance_tokens,
                "verification_email_code": verification,
                "password_reset_code": reset_code,
                "oauth_state": state,
                "oauth_pkce_challenge": challenge,
                "oauth_client_secret": f.oauth_secret,
                "captcha_response": captcha_response,
                "captcha_server_secret": f.captcha_secret,
                "new_password": new_password,
                "actual_password_verifier": actual_hash,
                "internal_auth_jwt": f.auth,
                "internal_shop_jwt": f.shop,
            }
            matches = {name: value in raw for name, value in known.items()}
            controls = {
                name: name in raw
                for name in [
                    "generate_auth_url",
                    "request_verification",
                    "request_password_reset",
                    "reset_password",
                    "siteverify",
                    "VerifyEmailTemplate",
                    "ResetPasswordTemplate",
                    "get_download_token",
                    "download_invoice",
                    "download_credit_note",
                    "/finance/invoices/{token}/{invoice_number}/invoice.pdf",
                    "/finance/credit_notes/{token}/{year}/{month}/credit_note.pdf",
                    "<unmatched>",
                    "finished processing request",
                    "academy_core_admin_audit_impl",
                    "academy_persistence_postgres::admin_audit",
                ]
            }
            controls.update({name: name in raw for name in ["academy_auth_impl::internal"]})
            (OUT / (build + "-controls.json")).write_text(
                json.dumps({"controls": controls, "matches": matches}, indent=2) + "\n"
            )
            assert all(controls.values()), "a positive TRACE control was missing"
            delivered_codes = re.findall(r"\b[A-Z0-9]{4}(?:-[A-Z0-9]{4}){3}\b", "\n".join(f.smtp.bodies))
            assert len(set(delivered_codes)) >= 2, "SMTP must deliver both actual codes"
            matches["all_delivered_mail_codes"] = any(code in raw for code in delivered_codes)
            legacy = {key: matches.pop(key) for key in ("new_password", "actual_password_verifier")}
            base = f.base
            row = {
                "build": build,
                "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "credential_matches": matches,
                "password_hash_session_comparisons": {
                    **legacy,
                    "issued_session_token_matches": any(token in raw for token in f.tokens),
                },
                "issued_internal_tokens_compared": len(f.peer.tokens),
                "issued_internal_audiences_compared": sorted(audiences),
                "issued_internal_token_matches": any(token in raw for token in f.peer.tokens),
                "positive_trace_controls": controls,
                "smtp_messages": f.smtp.messages,
                "successful_email_verification": True,
                "successful_password_reset_and_login": True,
                "oauth_begin_succeeded": True,
                "captcha_loopback_verified": True,
                "finance": {
                    "issued_bearers_compared": 2,
                    "owned_invoice_downloaded_without_authorization_header": True,
                    "owned_credit_note_downloaded_without_authorization_header": True,
                    "downloaded_bytes_match_originals": True,
                    "encoded_bearer_download_status": 200,
                    "invalid_invoice_bearer_status": 401,
                    "invalid_credit_note_bearer_status": 401,
                    "foreign_recipient_status": 404,
                    "wrong_audience_bearer_status": 401,
                    "query_bearer_download_status": 200,
                    "path_parse_error_status": 400,
                    "unmatched_route_status": 404,
                    "admin_audit": finance_audit,
                },
                "raw_trace_and_smtp_persisted": False,
            }
        row["fixture_removed"] = not base.exists()
        row["trace_readers_stopped"] = all(not thread.is_alive() for _, thread in f.captures)
        report["checks"].append(row)
        (OUT / "result.json").write_text(json.dumps(report, indent=2) + "\n")
    report["credential_secrecy_passed"] = all(
        not any(row["credential_matches"].values())
        and not any(row["password_hash_session_comparisons"].values())
        and not row["issued_internal_token_matches"]
        and not row["finance"]["admin_audit"]["bearer_matches"]
        and row["finance"]["admin_audit"]["paths_redacted"]
        for row in report["checks"]
    )
    report["passed"] = report["credential_secrecy_passed"] and all(
        row["fixture_removed"] and row["trace_readers_stopped"] for row in report["checks"]
    )
    report["status"] = "passed" if report["passed"] else "failed"
    assert report["passed"], "scoped secret present in real TRACE; values omitted"
except BaseException as error:
    report["status"] = "failed"
    report["error_type"] = type(error).__name__
    raise
finally:
    (OUT / "result.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({"status": report["status"], "profiles": len(report["checks"])}))
