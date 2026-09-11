"""Actual internal/admin/one-shot task renewal restriction and row-lock race.
All users, wallet funding, time changes and SMTP acceptance are synthetic.
"""

import asyncio, importlib.util, json, threading, subprocess, time
from pathlib import Path
from uuid import UUID, uuid4
import asyncpg, httpx, jwt

spec = importlib.util.spec_from_file_location("fixture", Path(__file__).with_name("moderation-http.py"))
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)


class AcceptedSMTP(f.SMTPHandler):
    def handle(self):
        self.wfile.write(b"220 synthetic SMTP\r\n")
        while line := self.rfile.readline():
            verb = line.split(b" ", 1)[0].strip().upper()
            if verb in [b"EHLO", b"HELO"]:
                self.wfile.write(b"250-localhost\r\n250 8BITMIME\r\n")
            elif verb == b"DATA":
                self.wfile.write(b"354 data\r\n")
                while (line := self.rfile.readline()) not in [b".\r\n", b""]:
                    pass
                self.wfile.write(b"250 synthetic accepted\r\n")
            elif verb == b"QUIT":
                self.wfile.write(b"221 bye\r\n")
                return
            else:
                self.wfile.write(b"250 OK\r\n")


async def main():
    smtp = f.SMTP(("127.0.0.1", 55903), AcceptedSMTP)
    threading.Thread(target=smtp.serve_forever, daemon=True).start()
    db = monitor = None
    try:
        f.start(
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
            "renewal-valkey.log",
        )
        await f.wait_port(55902)
        f.start([str(f.ROOT / "target/debug/academy"), "serve"], "renewal-backend.log")
        await f.wait_port(55901)
        db = await asyncpg.connect("postgresql://l2test@127.0.0.1:55900/l2backend")
        monitor = await asyncpg.connect("postgresql://l2test@127.0.0.1:55900/l2backend")
        async with httpx.AsyncClient(base_url="http://127.0.0.1:55901", timeout=15) as c:

            async def cli(*args):
                process = await asyncio.create_subprocess_exec(
                    str(f.ROOT / "target/debug/academy"),
                    *args,
                    cwd=f.ROOT,
                    env=f.ENV,
                    stdout=asyncio.subprocess.PIPE,
                    stderr=asyncio.subprocess.STDOUT,
                )
                out = await process.communicate()
                assert process.returncode == 0, out

            name = "renew_" + uuid4().hex[:10]
            await cli("admin", "user", "create", "--verified", name, name + "@example.invalid", "synthetic password")
            login = await c.post("/auth/sessions", json={"name_or_email": name, "password": "synthetic password"})
            assert login.status_code == 200, login.text
            uid = UUID(login.json()["user"]["id"])
            user = {"Authorization": "Bearer " + login.json()["access_token"]}
            await cli("admin", "coin", "add", str(uid), "--", "50000")
            # Existing paid period fixture, not a new sale through the retired pre-L1 route.
            await db.execute(
                "INSERT INTO premium(id,user_id,since,until) VALUES($1,$2,clock_timestamp(),clock_timestamp()+interval '1 month')",
                uuid4(),
                uid,
            )
            offer = (await c.get("/shop/premium/renewal-offer")).json()
            assert offer["terms_version"] == "2026-09-r2"
            ordered = await c.put(
                "/shop/premium/autopay",
                headers=user,
                json={
                    "plan": "MONTHLY",
                    "consent": {
                        "request_id": str(uuid4()),
                        "offer_id": offer["id"],
                        "accepted": True,
                        "withdrawal_consent": True,
                    },
                },
            )
            assert ordered.status_code == 200, ordered.text
            assert await db.fetchval(
                "SELECT EXISTS(SELECT 1 FROM premium_renewal_delivery d JOIN premium_renewal_agreements a ON a.id=d.agreement_id WHERE a.user_id=$1 AND d.sent_at<a.confirmation_deadline)",
                uid,
            )
            admin = await c.post(
                "/auth/sessions",
                json={"name_or_email": "admin2", "password": "secure admin2 password", "mfa_code": f.totp()},
            )
            assert admin.status_code == 200, admin.text
            staff = {"Authorization": "Bearer " + admin.json()["access_token"]}
            actor = UUID(admin.json()["user"]["id"])
            internal = {
                "Authorization": "Bearer "
                + jwt.encode(
                    {"aud": "shop", "exp": int(time.time()) + 900}, "synthetic-l2-shop-internal", algorithm="HS256"
                )
            }

            async def snapshot():
                return await db.fetchval(
                    "SELECT jsonb_build_object('coins',(SELECT to_jsonb(c) FROM coins c WHERE user_id=$1),'periods',(SELECT jsonb_agg(to_jsonb(p) ORDER BY id) FROM premium p WHERE user_id=$1),'subscriptions',(SELECT jsonb_agg(to_jsonb(s)) FROM premium_subscriptions s WHERE user_id=$1),'agreements',(SELECT jsonb_agg(to_jsonb(a)) FROM premium_renewal_agreements a WHERE user_id=$1),'ledger',(SELECT jsonb_agg(to_jsonb(t)) FROM transactions t WHERE user_id=$1))::text",
                    uid,
                )

            case = uuid4()
            await db.fetchval(
                "SELECT backend_moderation('open',$1,$2::jsonb)",
                actor,
                json.dumps(
                    {
                        "id": str(case),
                        "target_id": str(uid),
                        "source": "own_review",
                        "private_evidence": {"facts": "Synthetic renewal pause"},
                    }
                ),
            )
            command = {
                "case_id": str(case),
                "request_key": str(uuid4()),
                "expected_revision": 0,
                "outcome": "restrict",
                "rationale": "Synthetic restriction",
                "ground": "Synthetic security ground",
                "rule_version": "Synthetic only",
                "automation": "Synthetic human command",
                "scope": "Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten",
                "redress": "At least six calendar months human review",
                "misconduct_facts": "Synthetic actual security evidence",
                "proportionality": "Synthetic proportionate response",
                "hearing": "Synthetic urgent assessment",
            }
            before = await snapshot()
            await db.fetchval("SELECT backend_moderation('decide',$1,$2::jsonb)", actor, json.dumps(command))
            status = await c.get("/shop/premium/" + str(uid), headers=staff)
            assert status.status_code == 200 and status.json()["premium"] and status.json()["renewal"]
            assert (await c.get("/shop/_internal/premium/" + str(uid), headers=internal)).json() is True
            assert await snapshot() == before
            print(
                "PASS restriction preserves current paid period, wallet/ledger and genuine renewal agreement through actual admin/internal reads",
                flush=True,
            )
            # Fixture time change to the paid row only; no machine clock changes.
            await db.execute(
                "UPDATE premium SET since=clock_timestamp()-interval '2 months',until=clock_timestamp()-interval '1 month' WHERE user_id=$1",
                uid,
            )
            before = await snapshot()
            for path, h in [("/shop/premium/" + str(uid), staff), ("/shop/_internal/premium/" + str(uid), internal)]:
                r = await c.get(path, headers=h)
                assert r.status_code == 200, r.text
            await cli("task", "refresh-premium")
            assert await snapshot() == before
            print(
                "PASS expired restricted subscription: actual internal/admin reads and one-shot refresh task produce no debit, extension, cancellation or agreement mutation",
                flush=True,
            )
            restore = command | {"request_key": str(uuid4()), "expected_revision": 1, "outcome": "restore"}
            await db.fetchval("SELECT backend_moderation('decide',$1,$2::jsonb)", actor, json.dumps(restore))
            # Let the actual internal call pass its existence read and block on the user
            # row. Commit a restriction first; it must re-read enabled under that lock.
            tx = db.transaction()
            await tx.start()
            await db.fetchval("SELECT id FROM users WHERE id=$1 FOR UPDATE", uid)
            pending = asyncio.create_task(c.get("/shop/_internal/premium/" + str(uid), headers=internal))
            for _ in range(100):
                blocked = await monitor.fetchval(
                    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname='l2backend' AND wait_event_type='Lock' AND lower(query) LIKE '%users%for update%')"
                )
                if blocked:
                    break
                await asyncio.sleep(0.03)
            assert blocked and not pending.done()
            command |= {"request_key": str(uuid4()), "expected_revision": 2}
            await db.fetchval("SELECT backend_moderation('decide',$1,$2::jsonb)", actor, json.dumps(command))
            await tx.commit()
            r = await pending
            assert r.status_code == 200 and r.json() is False, r.text
            assert await snapshot() == before
            print(
                "PASS actual internal pre-read vs committed restriction race: restriction wins user-row lock, later renewal sees disabled and cannot debit",
                flush=True,
            )
            restored = command | {"request_key": str(uuid4()), "expected_revision": 3, "outcome": "restore"}
            await db.fetchval("SELECT backend_moderation('decide',$1,$2::jsonb)", actor, json.dumps(restored))
            old_coins = await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", uid)
            r = await c.get("/shop/_internal/premium/" + str(uid), headers=internal)
            assert r.status_code == 200 and r.json() is True, r.text
            assert (
                await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", uid) == old_coins - offer["monthly_price"]
            )
            assert await db.fetchval(
                "SELECT max(since)>clock_timestamp()-interval '1 minute' FROM premium WHERE user_id=$1", uid
            ), "restoration charged retroactive catch-up"
            await cli("task", "refresh-premium")
            assert (
                await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", uid) == old_coins - offer["monthly_price"]
            )
            print(
                "PASS restored actual confirmed agreement renews once from present debit time; no retroactive period or repeated catch-up charge",
                flush=True,
            )
    finally:
        for conn in [db, monitor]:
            if conn:
                await conn.close()
        for p in reversed(f.CHILDREN):
            if p.poll() is None:
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
