"""Compare actual credentials to real HTTP TRACE in debug and release builds.

Reuses backend-safety.py's owned PostgreSQL/Valkey/SMTP fixture. Backend logs
stay in memory; evidence contains boolean comparisons and binary/source hashes.
"""

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import threading

REPO = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("auth_trace_fixture", REPO / "tests/backend-safety.py")
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


def probe(args, build, binary):
    owned = argparse.Namespace(
        binary=binary.resolve(), pg_bin=args.pg_bin.resolve(), valkey=args.valkey.resolve(), output=args.output / build
    )
    with TraceFixture(owned) as f:
        user = f.account(admin=True)
        password_hash = f.sql(f"SELECT password_hash FROM user_passwords WHERE user_id='{user['id']}'")
        assert password_hash.startswith("$argon2id$"), "fixture did not read its real password verifier"
        secret = f.request("/auth/users/me/mfa", "POST", token=f.token(user))
        confirmation = fixture.totp(secret)
        recovery = f.request("/auth/users/me/mfa", "PUT", {"code": confirmation}, f.token(user))
        code = fixture.totp(secret, offset=1)
        login = f.login(user["name"], user["password"], mfa_code=code)
        assert login["session"]["mfa_verified"] is True
        replay, _ = f.request(
            "/auth/sessions",
            "POST",
            {"name_or_email": user["name"], "password": user["password"], "mfa_code": code},
            expected=None,
        )
        assert replay == 412, "consumed TOTP authenticated again"
        wrong = "owned-incorrect-password-trace-probe"
        denied, _ = f.request(
            "/auth/sessions",
            "POST",
            {"name_or_email": user["name"], "password": wrong, "mfa_code": code},
            expected=None,
        )
        assert denied == 401, "wrong password authenticated"
        refreshed = f.request("/auth/session", "PUT", {"refresh_token": login["refresh_token"]})
        assert refreshed["session"]["mfa_verified"] is True
        recovered = f.login(user["name"], user["password"], recovery_code=recovery)
        assert recovered["session"]["mfa_verified"] is False
        assert f.sql(f"SELECT count(*) FROM totp_devices WHERE user_id='{user['id']}'") == "0"
        f.request("/auth/users", token=refreshed["access_token"], expected=401)
        f.login(user["name"], user["password"])
        raw_secret = base64.b32decode(secret + "=" * (-len(secret) % 8))
        values = {
            "password": [user["password"], wrong],
            "stored_argon2_verifier": [password_hash],
            "totp_codes": [confirmation, code],
            "totp_secret": [secret, "[" + ", ".join(map(str, raw_secret)) + "]"],
            "recovery_code": [recovery],
            "all_issued_session_tokens": list(f.tokens),
            "internal_tokens": [f.auth, f.shop],
        }
        base = f.base
        f.backend.terminate()
        f.backend.wait(timeout=15)
        for _, thread in f.captures:
            thread.join(timeout=15)
            assert not thread.is_alive(), "TRACE drain did not complete"
        raw = b"\n".join(chunks for chunks, _ in f.captures).decode(errors="replace")
        raw = re.sub(r"\x1b\[[0-9;]*m", "", raw)
        controls = {
            name: name in raw
            for name in (
                "create_user",
                "save_password_hash",
                "get_password_hash",
                "verify",
                "generate_secret",
                "consume_totp_step",
                "generate_mfa_recovery_code",
                "enable",
                "create_session",
                "refresh_session",
                "try recovery code",
                "recovery code matches",
            )
        }
        matches = {name: any(value in raw for value in known) for name, known in values.items()}
        # Do not persist the captured logs, credentials, or matching substrings.
        result = {
            "build": build,
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "positive_trace_controls": controls,
            "credential_matches": matches,
            "issued_tokens_compared": len(f.tokens),
            "replay_status": replay,
            "wrong_password_status": denied,
            "raw_trace_persisted": False,
            "passed": all(controls.values()) and not any(matches.values()),
        }
    result["fixture_removed"] = not base.exists()
    result["trace_readers_stopped"] = all(not thread.is_alive() for _, thread in f.captures)
    result["passed"] = result["passed"] and result["fixture_removed"] and result["trace_readers_stopped"]
    return result


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--debug-binary", type=Path, required=True)
    parser.add_argument("--release-binary", type=Path, required=True)
    parser.add_argument("--pg-bin", type=Path, required=True)
    parser.add_argument("--valkey", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    report = {
        "started_at": datetime.now(timezone.utc).isoformat(),
        "passed": False,
        "checks": [],
        "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "fixture_sha256": hashlib.sha256((REPO / "tests/backend-safety.py").read_bytes()).hexdigest(),
    }
    try:
        assert args.debug_binary.resolve() != args.release_binary.resolve(), "two separate build artifacts required"
        for name, binary in [("debug", args.debug_binary), ("release", args.release_binary)]:
            result = probe(args, name, binary)
            report["checks"].append(result)
            print(name + ": " + str(result["passed"]), flush=True)
        report["passed"] = all(row["passed"] for row in report["checks"])
    except BaseException as error:
        # Some fixture exceptions may contain response data. Keep only the type.
        report["error_type"] = type(error).__name__
    finally:
        report["completed_at"] = datetime.now(timezone.utc).isoformat()
        (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
    print("TRACE confidentiality: " + str(report["passed"]), flush=True)
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
