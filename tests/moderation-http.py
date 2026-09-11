"""Opt-in local L2 fixture. Requires an owned /tmp/bootstrap-l2-implementation
PostgreSQL cluster migrated/seeded with the synthetic demo only. No external URLs.
"""

import asyncio, base64, hashlib, hmac, json, os, socket, socketserver, struct, subprocess, threading, time
from pathlib import Path
from uuid import UUID, uuid4
import asyncpg, httpx

ROOT = Path(__file__).resolve().parents[1]
BASE = Path(os.environ["L2_BASE"]).resolve()
assert str(BASE) == "/tmp/bootstrap-l2-implementation"
ENV = os.environ | {"ACADEMY_CONFIG": f"{BASE}/backend.toml:{ROOT}/config.dev.toml", "RUST_LOG": "warn"}
CHILDREN = []


class SMTPHandler(socketserver.StreamRequestHandler):
    def handle(self):
        self.wfile.write(b"220 synthetic L2 SMTP\r\n")
        while line := self.rfile.readline():
            verb = line.split(b" ", 1)[0].strip().upper()
            if verb in [b"EHLO", b"HELO"]:
                self.wfile.write(b"250-localhost\r\n250 8BITMIME\r\n")
            elif verb == b"DATA":
                self.wfile.write(b"354 data\r\n")
                while (line := self.rfile.readline()) not in [b".\r\n", b""]:
                    pass
                self.wfile.write(b"451 synthetic outage; nothing delivered\r\n")
            elif verb == b"QUIT":
                self.wfile.write(b"221 bye\r\n")
                return
            else:
                self.wfile.write(b"250 OK\r\n")


class SMTP(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


def start(cmd, log):
    p = subprocess.Popen(cmd, cwd=ROOT, env=ENV, stdout=(BASE / log).open("w"), stderr=subprocess.STDOUT)
    CHILDREN.append(p)
    return p


async def wait_port(port):
    for _ in range(200):
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.1):
                return
        except OSError:
            await asyncio.sleep(0.05)
    raise AssertionError(f"fixture port {port} unavailable")


def totp():
    raw = hmac.new(
        base64.b32decode("CF3ABXI2PIN5AIKTFBWHTSMA24======"), struct.pack(">Q", int(time.time()) // 30), hashlib.sha1
    ).digest()
    offset = raw[-1] & 15
    return f'{(int.from_bytes(raw[offset:offset+4],"big")&0x7fffffff)%1000000:06}'


async def main():
    smtp = SMTP(("127.0.0.1", 55903), SMTPHandler)
    threading.Thread(target=smtp.serve_forever, daemon=True).start()
    try:
        start(
            [
                "/nix/store/d4lznfvcd8zqxn4hc9lpw0dvfri8p4c0-valkey-9.1.1/bin/valkey-server",
                "--port",
                "55902",
                "--bind",
                "127.0.0.1",
                "--save",
                "",
                "--appendonly",
                "no",
            ],
            "valkey.log",
        )
        await wait_port(55902)
        start([str(ROOT / "target/debug/academy"), "serve"], "backend-http.log")
        await wait_port(55901)
        db = await asyncpg.connect("postgresql://l2test@127.0.0.1:55900/l2backend")
        async with httpx.AsyncClient(base_url="http://127.0.0.1:55901", timeout=20) as client:

            async def login(name, password, **extra):
                r = await client.post("/auth/sessions", json={"name_or_email": name, "password": password, **extra})
                assert r.status_code == 200, r.text
                return r.json()

            foo = await login("foo", "foo password")
            admin = await login("admin2", "secure admin2 password", mfa_code=totp())
            assert admin["session"]["mfa_verified"]
            assert len(foo["access_token"].split(".")) == 3
            uid = foo["user"]["id"]
            headers = {"Authorization": "Bearer " + foo["access_token"]}
            staff = {"Authorization": "Bearer " + admin["access_token"]}
            # Internal audience token has no ordinary recipient authority.
            # Internal token uses the configured audience secret, as service transports do.
            import jwt

            internal = jwt.encode(
                {"aud": "auth", "exp": int(time.time()) + 900}, "synthetic-l2-auth-internal", algorithm="HS256"
            )
            ih = {"Authorization": "Bearer " + internal}
            before = await client.post(
                "/auth/_internal/ordinary-authority", headers=ih, json={"access_token": foo["access_token"]}
            )
            assert before.status_code == 200, before.text
            assert before.json()["id"] == uid
            case = str(uuid4())
            r = await client.post(
                "/auth/moderation/admin/open",
                headers=staff,
                json={
                    "id": case,
                    "target_id": uid,
                    "source": "own_review",
                    "private_evidence": {
                        "facts": "Synthetic only; no real customer finding",
                        "private_marker": "DO NOT DISCLOSE",
                    },
                },
            )
            assert r.status_code == 200, r.text
            cmd = {
                "request_key": str(uuid4()),
                "case_id": case,
                "expected_revision": 0,
                "outcome": "restrict",
                "misconduct_facts": "Synthetic independently verified account security finding",
                "proportionality": "Synthetic necessary limited restriction",
                "hearing": "Synthetic concrete immediate urgency assessed",
                "rationale": "Synthetic restriction for acceptance testing. No real-person finding.",
                "ground": "Synthetic test ground",
                "rule_version": "Synthetic rules, not a legal-capacity finding",
                "automation": "Human test command; no automatic merits finding",
                "scope": "Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten",
                "redress": "Six months human review and other legal remedies",
            }
            r = await client.post("/auth/moderation/admin/decide", headers=staff, json=cmd)
            assert r.status_code == 200, r.text
            decision = r.json()
            assert (await client.post("/auth/moderation/admin/decide", headers=staff, json=cmd)).json() == decision
            assert not await db.fetchval("SELECT enabled FROM users WHERE id=$1", UUID(uid))
            assert await db.fetchval("SELECT count(*) FROM sessions WHERE user_id=$1", UUID(uid)) == 0
            assert (await client.get("/auth/session", headers=headers)).status_code == 401
            assert (await client.put("/auth/session", json={"refresh_token": foo["refresh_token"]})).status_code == 401
            assert (await client.post("/auth/sessions/" + uid, headers=staff)).status_code == 404
            assert (
                await client.post(
                    "/auth/_internal/ordinary-authority", headers=ih, json={"access_token": foo["access_token"]}
                )
            ).status_code == 401
            identity = await client.get("/auth/_internal/users/" + uid, headers=ih)
            assert identity.status_code == 200 and identity.json()["enabled"] is False
            print(
                "PASS producer-issued ordinary token/refresh/impersonation denied after committed restriction; disabled internal identity remains200",
                flush=True,
            )
            proof = await client.post(
                "/auth/moderation/access/password", json={"name_or_email": "foo", "password": "foo password"}
            )
            assert proof.status_code == 200, proof.text
            capability = proof.json()["capability"]
            assert "." not in capability
            rights = {"x-moderation-capability": capability}
            assert (
                await client.get("/auth/session", headers={"Authorization": "Bearer " + capability})
            ).status_code == 401
            inbox = await client.get("/auth/moderation/inbox", headers=rights)
            assert inbox.status_code == 200, inbox.text
            assert "DO NOT DISCLOSE" not in inbox.text and decision["decision_id"] in inbox.text
            message = next(m for m in inbox.json()["backend"] if m["decision_id"] == decision["decision_id"])
            assert message["informed_at"] is None and message["complaint_until"] is None
            for _ in range(90):
                state = await db.fetchval(
                    "SELECT status FROM moderation_delivery WHERE source='backend' AND id=$1", UUID(message["id"])
                )
                if state == "uncertain":
                    break
                await asyncio.sleep(0.5)
            assert state == "uncertain", f"Synthetic SMTP failure not recorded: {state}"
            assert await db.fetchval(
                "SELECT informed_at IS NULL AND relayed_at IS NOT NULL FROM moderation_messages WHERE id=$1",
                UUID(message["id"]),
            )
            assert (
                await client.post("/auth/moderation/opened/backend", headers=rights, json={"id": message["id"]})
            ).json() is True
            assert await db.fetchval(
                "SELECT informed_at IS NOT NULL AND complaint_until IS NOT NULL FROM moderation_messages WHERE id=$1",
                UUID(message["id"]),
            )
            assert (
                await client.post("/auth/moderation/opened/backend", headers=rights, json={"id": str(uuid4())})
            ).status_code == 403
            finance = await client.post("/auth/moderation/finance-access", headers=rights)
            assert finance.status_code == 200, finance.text
            assert (
                await client.get("/auth/session", headers={"Authorization": "Bearer " + finance.json()["token"]})
            ).status_code == 401
            print(
                "PASS real relay worker retains SMTP failure as uncertainty; relay acceptance does not start review clock; actual recipient opening starts recorded minimum; dedicated finance token is not ordinary authority",
                flush=True,
            )
            exported = await client.get("/auth/moderation/export", headers=rights)
            assert exported.status_code == 200, exported.text
            assert exported.json()["account"]["user"]["enabled"] is False
            assert await db.fetchval("SELECT count(*) FROM sessions WHERE user_id=$1", UUID(uid)) == 0
            complaint = {
                "id": str(uuid4()),
                "decision_id": decision["decision_id"],
                "text": "Synthetic appeal requesting human review.",
            }
            r = await client.post("/auth/moderation/complaints/backend", headers=rights, json=complaint)
            assert r.status_code == 200, r.text
            assert (
                await client.post("/auth/moderation/complaints/backend", headers=rights, json=complaint)
            ).status_code == 200
            assert (
                await client.post("/auth/moderation/complaints/backend", headers=rights, json={"id": complaint["id"]})
            ).status_code == 409
            print(
                "PASS disabled credential proof grants only opaque rights capability; immediate safe reasons, export and durable replayable human appeal without ordinary session creation",
                flush=True,
            )
            # Cache loss cannot revive the deleted session, and no cache invalidation is
            # needed to enforce the committed database boundary.
            import redis.asyncio as redis

            cache = redis.from_url("redis://127.0.0.1:55902/0")
            await cache.flushdb()
            await cache.aclose()
            assert (
                await client.post(
                    "/auth/_internal/ordinary-authority", headers=ih, json={"access_token": foo["access_token"]}
                )
            ).status_code == 401
            assert (await client.patch("/auth/users/" + uid, headers=staff, json={"enabled": True})).status_code == 409
            assert (await client.delete("/auth/users/" + uid, headers=staff)).status_code == 409
            # Save only synthetic tokens to the explicitly owned private fixture for
            # subsequent direct consumer tests, not to evidence or application logs.
            (BASE / "consumer-tokens.json").write_text(
                json.dumps({"restricted": foo, "admin": admin, "rights": capability})
            )
            (BASE / "consumer-tokens.json").chmod(0o600)
            print(
                "PASS Redis flush does not revive authority; direct enabled PATCH and administrative erasure rejected",
                flush=True,
            )
        await db.close()
    finally:
        for p in reversed(CHILDREN):
            p.terminate()
            try:
                p.wait(timeout=15)
            except subprocess.TimeoutExpired:
                p.kill()
                p.wait()
        smtp.shutdown()
        smtp.server_close()


if __name__ == "__main__":
    asyncio.run(main())
