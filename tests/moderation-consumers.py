"""Opt-in actual five-service synthetic auth boundary, all URLs loopback.
Requires owned migrated L2 backend/challenges databases and local binaries.
"""

import asyncio, importlib.util, json, os, subprocess, threading, time
from pathlib import Path
from uuid import uuid4, UUID
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import asyncpg, httpx, jwt

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("fixture", HERE / "moderation-http.py")
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)
BASE = f.BASE
ROOT = f.ROOT
WORK = ROOT.parent
PY = WORK / "skills-ms/.venv/bin/python"


class Sandbox(BaseHTTPRequestHandler):
    def do_GET(self):
        body = b'{"openapi":"3.0.0","info":{"title":"Synthetic","version":"0.2.2"},"paths":{}}'
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


def service_env(name, port, index):
    return os.environ | {
        "PYTHONPATH": str(WORK / name),
        "JWT_SECRET": "synthetic-l2-only-ordinary-token-secret",
        "AUTH_URL": "http://127.0.0.1:55901/auth/",
        "SHOP_URL": "http://127.0.0.1:55901/shop/",
        "SKILLS_URL": "http://127.0.0.1:55906/",
        "EVENTS_URL": "http://127.0.0.1:55905/",
        "REDIS_URL": f"redis://127.0.0.1:55902/{index}",
        "AUTH_REDIS_URL": "redis://127.0.0.1:55902/0",
        "DATABASE_URL": f'postgresql+asyncpg://l2test@127.0.0.1:55900/l2{name.split("-")[0]}',
        "INTERNAL_JWT_SECRET_AUTH": "synthetic-l2-auth-internal",
        "INTERNAL_JWT_SECRET_SHOP": "synthetic-l2-shop-internal",
        "INTERNAL_JWT_SECRET_SKILLS": "synthetic-l2-skills-internal",
        "INTERNAL_JWT_SECRET_EVENTS": "synthetic-l2-events-internal",
        "INTERNAL_JWT_SECRET_JOBS": "synthetic-l2-jobs-internal",
        "SMTP_HOST": "127.0.0.1",
        "SMTP_PORT": "55903",
        "SMTP_FROM": "sender@example.invalid",
        "SMTP_STARTTLS": "false",
        "SMTP_TLS": "false",
        "POOL_SIZE": "2",
        "MAX_OVERFLOW": "0",
        "PORT": str(port),
        "HOST": "127.0.0.1",
        "SENTRY_DSN": "",
        "LOG_LEVEL": "WARNING",
    }


async def main():
    smtp = f.SMTP(("127.0.0.1", 55903), f.SMTPHandler)
    threading.Thread(target=smtp.serve_forever, daemon=True).start()
    sandbox = ThreadingHTTPServer(("127.0.0.1", 55907), Sandbox)
    threading.Thread(target=sandbox.serve_forever, daemon=True).start()
    try:
        cache = f.start(
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
            "consumer-valkey.log",
        )
        await f.wait_port(55902)
        backend = f.start([str(ROOT / "target/debug/academy"), "serve"], "consumer-backend.log")
        await f.wait_port(55901)
        for name, port, index in [("skills-ms", 55906, 1), ("events-ms", 55905, 4), ("jobs-ms", 55908, 3)]:
            env = service_env(name, port, index)
            service_python = WORK / "events-ms/.venv/bin/python" if name == "events-ms" else PY
            init = "import asyncio\nfrom api import models\nfrom api.database import db\nasync def main():\n await db.create_tables()\n await db.engine.dispose()\nasyncio.run(main())\n"
            subprocess.run(
                [str(service_python), "-c", init],
                cwd=WORK / name,
                env=env,
                check=True,
                stdout=(BASE / (name + "-schema.log")).open("w"),
                stderr=subprocess.STDOUT,
            )
            process = subprocess.Popen(
                [
                    str(service_python),
                    "-m",
                    "uvicorn",
                    "api.app:app",
                    "--host",
                    "127.0.0.1",
                    "--port",
                    str(port),
                    "--lifespan",
                    "off",
                    "--no-access-log",
                ],
                cwd=WORK / name,
                env=env,
                stdout=(BASE / (name + "-http.log")).open("w"),
                stderr=subprocess.STDOUT,
            )
            f.CHILDREN.append(process)
            await f.wait_port(port)
        db = await asyncpg.connect("postgresql://l2test@127.0.0.1:55900/l2backend")
        skills = await asyncpg.connect("postgresql://l2test@127.0.0.1:55900/l2skills")
        await skills.execute(
            "INSERT INTO skills_root_skill(id,name,row,\"column\",sub_tree_rows,sub_tree_columns) VALUES('l2-root','Synthetic skill',0,0,1,1) ON CONFLICT DO NOTHING"
        )
        await skills.execute(
            "INSERT INTO skills_sub_skill(id,parent_id,name,row,\"column\") VALUES('l2-sub','l2-root','Synthetic subskill',0,0) ON CONFLICT DO NOTHING"
        )
        await skills.close()
        env = os.environ | {"CONFIG_PATH": str(BASE / "challenges.toml"), "RUST_LOG": "warn"}
        process = subprocess.Popen(
            [str(WORK / "challenges-ms/target/debug/challenges")],
            cwd=WORK / "challenges-ms",
            env=env,
            stdout=(BASE / "consumer-challenges.log").open("w"),
            stderr=subprocess.STDOUT,
        )
        f.CHILDREN.append(process)
        await f.wait_port(55904)
        async with httpx.AsyncClient(timeout=15) as http:

            async def backend_call(method, path, **kw):
                return await http.request(method, "http://127.0.0.1:55901" + path, **kw)

            async def login(name, password, **kw):
                r = await backend_call(
                    "POST", "/auth/sessions", json={"name_or_email": name, "password": password, **kw}
                )
                assert r.status_code == 200, r.text
                return r.json()

            foo = await login("foo", "foo password")
            admin = await login("admin2", "secure admin2 password", mfa_code=f.totp())
            staff = {"Authorization": "Bearer " + admin["access_token"]}
            user = {"Authorization": "Bearer " + foo["access_token"]}
            uid = foo["user"]["id"]
            check_internal = jwt.encode(
                {"aud": "auth", "exp": int(time.time()) + 300}, "synthetic-l2-auth-internal", algorithm="HS256"
            )
            checked = await backend_call(
                "POST",
                "/auth/_internal/ordinary-authority",
                headers={"Authorization": check_internal},
                json={"access_token": admin["access_token"]},
            )
            assert checked.status_code == 200, (checked.status_code, checked.text)
            print("PASS direct backend ordinary-authority preflight", flush=True)
            company = await http.post(
                "http://127.0.0.1:55908/companies", headers=staff, json={"name": "Synthetic L2 " + str(uuid4())}
            )
            assert company.status_code == 200, company.text
            job = await http.post(
                "http://127.0.0.1:55908/jobs",
                headers=staff,
                json={
                    "company_id": company.json()["id"],
                    "title": "Synthetic role",
                    "description": "Synthetic only",
                    "location": "Synthetic",
                    "remote": True,
                    "type": "full_time",
                    "responsibilities": [],
                    "professional_level": "entry",
                    "salary": {"min": 0, "max": 0, "unit": "EUR", "per": "year"},
                    "contact": "PRIVATE SYNTHETIC JOB CONTACT",
                    "skill_requirements": {},
                },
            )
            assert job.status_code == 200, job.text
            jobid = job.json()["id"]

            async def probes(headers, allowed):
                replies = await asyncio.gather(
                    http.post("http://127.0.0.1:55906/bookmark/l2-root/l2-sub", headers=headers),
                    http.post(
                        "http://127.0.0.1:55905/slots/me",
                        headers=headers,
                        json={"slots": [{"start": int(time.time()) + 86400, "duration": 30}]},
                    ),
                    http.get("http://127.0.0.1:55904/subtasks/user_config", headers=headers),
                    http.get("http://127.0.0.1:55908/jobs/" + jobid, headers=headers),
                )
                for i, r in enumerate(replies[:3]):
                    expected = (200, 409) if allowed and i == 0 else ((200,) if allowed else (401,))
                    assert r.status_code in expected, (i, r.status_code, r.text)
                    if r.status_code == 409:
                        assert r.json()["detail"] == "Skill already bookmarked"
                assert replies[3].status_code == 200, replies[3].text
                assert (replies[3].json()["contact"] is not None) == allowed, replies[3].text

            await probes(user, True)
            print(
                "PASS actual Skills bookmark, Events slot, Challenges user config and Jobs private-contact routes accept backend-produced ordinary token",
                flush=True,
            )
            # Deliberately retain warmed positive display/skill caches across restriction.
            case = str(uuid4())
            opened = await backend_call(
                "POST",
                "/auth/moderation/admin/open",
                headers=staff,
                json={
                    "id": case,
                    "target_id": uid,
                    "source": "own_review",
                    "private_evidence": {"facts": "Synthetic distributed auth test"},
                },
            )
            assert opened.status_code == 200, opened.text
            cmd = {
                "request_key": str(uuid4()),
                "case_id": case,
                "expected_revision": 0,
                "outcome": "restrict",
                "misconduct_facts": "Synthetic independently verified account security finding",
                "proportionality": "Synthetic necessary limited restriction",
                "hearing": "Synthetic concrete immediate urgency assessed",
                "rationale": "Synthetic restriction for distributed acceptance only.",
                "ground": "Synthetic ground",
                "rule_version": "Synthetic basis; no real-person finding",
                "automation": "Synthetic human command",
                "scope": "Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten",
                "redress": "Six months human review and other remedies",
            }
            decided = await backend_call("POST", "/auth/moderation/admin/decide", headers=staff, json=cmd)
            assert decided.status_code == 200, decided.text
            await probes(user, False)
            import redis.asyncio as redis

            rc = redis.from_url("redis://127.0.0.1:55902/0")
            await rc.flushdb()
            await probes(user, False)
            proof = await backend_call(
                "POST", "/auth/moderation/access/password", json={"name_or_email": "foo", "password": "foo password"}
            )
            assert proof.status_code == 200, proof.text
            await probes({"Authorization": "Bearer " + proof.json()["capability"]}, False)
            # Actual retained-rights call traverses backend -> Events -> internal shop.
            # The booked payment below is explicitly seeded synthetic historical evidence.
            events = await asyncpg.connect("postgresql://l2test@127.0.0.1:55900/l2events")

            async def booked(hours=240, participant=True):
                event, payment = str(uuid4()), str(uuid4())
                await events.execute(
                    """INSERT INTO events_webinars(id,skill_id,creator,creation_date,name,description,admin_link,link,start,"end",max_participants,price) VALUES($1,'l2-sub',$2,now(),'Synthetic rights webinar','Synthetic historical booking','','',now()+$3::interval,now()+$3::interval+interval '1 hour',10,999)""",
                    event,
                    admin["user"]["id"],
                    __import__("datetime").timedelta(hours=hours),
                )
                if participant:
                    await events.execute(
                        "INSERT INTO events_booking_payments(id,event_id,user_id,kind,state,quoted_coins,paid_coins,payout_coins,payout_ratio,description,created_at,original,evidence,attempts) VALUES($1,$2,$3,'webinar','paid',100,100,80,'0.8','Synthetic original booked payment',now(),'{}','{\"kind\":\"synthetic_original_fixture\"}',0)",
                        payment,
                        event,
                        uid,
                    )
                    await events.execute(
                        "INSERT INTO events_webinar_participants(webinar_id,user_id,paid_coins,payment_id) VALUES($1,$2,100,$3)",
                        event,
                        uid,
                        payment,
                    )
                return event

            event = await booked()
            past = await booked(12)
            foreign = await booked(participant=False)
            emails = await db.fetch(
                "SELECT id,email FROM users WHERE id=ANY($1::uuid[])", [UUID(uid), UUID(admin["user"]["id"])]
            )
            # No SMTP/DNS call for event notices; addresses are absent only in this fixture.
            await db.execute("UPDATE users SET email=NULL WHERE id=ANY($1::uuid[])", [r["id"] for r in emails])
            retained = {"x-moderation-capability": proof.json()["capability"]}
            before_coins = (await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", UUID(uid))) or 0
            cancelled = await backend_call("DELETE", "/auth/moderation/events/" + event, headers=retained)
            assert cancelled.status_code == 200, cancelled.text
            assert not await events.fetchval(
                "SELECT EXISTS(SELECT 1 FROM events_webinar_participants WHERE webinar_id=$1 AND user_id=$2)",
                event,
                uid,
            )
            assert await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", UUID(uid)) == before_coins + 100
            assert (
                await backend_call("DELETE", "/auth/moderation/events/" + event, headers=retained)
            ).status_code == 200
            assert await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", UUID(uid)) == before_coins + 100
            assert (
                await backend_call("DELETE", "/auth/moderation/events/" + past, headers=retained)
            ).status_code == 403
            assert (
                await backend_call("DELETE", "/auth/moderation/events/" + foreign, headers=retained)
            ).status_code == 404
            for row in emails:
                await db.execute("UPDATE users SET email=$2 WHERE id=$1", row["id"], row["email"])
            await events.close()
            print(
                "PASS actual disabled rights proof cancels only owned timely Events booking; original100coin payment basis credited once through internal shop despite current999price; replay stable, timing403/foreign404 preserved",
                flush=True,
            )

            internal = jwt.encode(
                {"aud": "auth", "exp": int(time.time()) + 300}, "synthetic-l2-auth-internal", algorithm="HS256"
            )
            identity = await backend_call(
                "GET", "/auth/_internal/users/" + uid, headers={"Authorization": "Bearer " + internal}
            )
            assert identity.status_code == 200 and identity.json()["enabled"] is False
            print(
                "PASS all four consumers reject/redact old token after committed restriction despite warm caches and Redis flush; opaque rights capability cannot become ordinary authority; disabled identity remains existing",
                flush=True,
            )
            restored = await backend_call(
                "POST",
                "/auth/moderation/admin/decide",
                headers=staff,
                json=cmd
                | {
                    "request_key": str(uuid4()),
                    "expected_revision": 1,
                    "outcome": "restore",
                    "rationale": "Synthetic restoration of this hold only.",
                },
            )
            assert restored.status_code == 200, restored.text
            await probes(user, False)
            fresh = await login("foo", "foo password")
            freshh = {"Authorization": "Bearer " + fresh["access_token"]}
            await probes(freshh, True)
            assert (await backend_call("DELETE", "/auth/session", headers=freshh)).status_code in [200, 204]
            decoded = jwt.decode(fresh["access_token"], options={"verify_signature": False})
            assert await rc.exists("access_token_invalidated:" + decoded["rt"])
            await rc.flushdb()
            await probes(freshh, False)
            print(
                "PASS restoration requires fresh session; enabled-account logout uses correct producer tombstone and stays revoked after tombstone loss",
                flush=True,
            )
            # Reachable backend infrastructure failure must produce unavailable, not401.
            cache.terminate()
            cache.wait(timeout=10)
            unreachable = await asyncio.gather(
                http.post("http://127.0.0.1:55906/bookmark/l2-root/l2-sub", headers=staff),
                *[
                    http.get(url, headers=staff)
                    for url in [
                        "http://127.0.0.1:55905/slots/me",
                        "http://127.0.0.1:55904/subtasks/user_config",
                        "http://127.0.0.1:55908/jobs/" + jobid,
                    ]
                ],
            )
            assert all(r.status_code == 503 for r in unreachable), [(r.status_code, r.text) for r in unreachable]
            authfail = await backend_call(
                "POST",
                "/auth/_internal/ordinary-authority",
                headers={"Authorization": "Bearer " + internal},
                json={"access_token": admin["access_token"]},
            )
            assert authfail.status_code == 503, authfail.text
            print(
                "PASS unavailable current authority fails closed as503 across all four consumers and reachable backend; no stale privileged response",
                flush=True,
            )
            await rc.aclose()
        await db.close()
    finally:
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
        sandbox.shutdown()
        sandbox.server_close()


if __name__ == "__main__":
    asyncio.run(main())
