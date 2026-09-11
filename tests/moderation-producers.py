"""Actual password/OAuth registration, durable session and restricted proof routes.
Uses the repository's real synthetic OAuth provider on an owned loopback port.
"""

import asyncio, importlib.util, json, threading, subprocess
from pathlib import Path
from urllib.parse import parse_qs, urlparse
from uuid import UUID, uuid4
import asyncpg, httpx

spec = importlib.util.spec_from_file_location("fixture", Path(__file__).with_name("moderation-http.py"))
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)


async def main():
    smtp = f.SMTP(("127.0.0.1", 55903), f.SMTPHandler)
    threading.Thread(target=smtp.serve_forever, daemon=True).start()
    db = None
    try:
        override = f.BASE / "oauth-producer.toml"
        override.write_text(
            '[oauth2.providers.test]\nauth_url="http://127.0.0.1:55907/oauth2/authorize"\ntoken_url="http://127.0.0.1:55907/oauth2/token"\nuserinfo_url="http://127.0.0.1:55907/user"\n'
        )
        f.ENV["ACADEMY_CONFIG"] = str(override) + ":" + f.ENV["ACADEMY_CONFIG"]
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
            "producer-valkey.log",
        )
        await f.wait_port(55902)
        f.start(
            [
                str(f.ROOT / "target/debug/academy-testing"),
                "oauth2",
                "--port",
                "55907",
                "--redirect-url",
                "http://127.0.0.1:55909/oauth/callback",
            ],
            "producer-provider.log",
        )
        await f.wait_port(55907)
        f.start([str(f.ROOT / "target/debug/academy"), "serve"], "producer-backend.log")
        await f.wait_port(55901)
        db = await asyncpg.connect("postgresql://l2test@127.0.0.1:55900/l2backend")
        async with httpx.AsyncClient(base_url="http://127.0.0.1:55901", timeout=20) as c:

            async def durable(login):
                uid = UUID(login["user"]["id"])
                sid = UUID(login["session"]["id"])
                assert await db.fetchval(
                    "SELECT EXISTS(SELECT 1 FROM sessions s JOIN session_refresh_tokens rt ON rt.session_id=s.id WHERE s.id=$1 AND s.user_id=$2)",
                    sid,
                    uid,
                )
                r = await c.get("/auth/session", headers={"Authorization": "Bearer " + login["access_token"]})
                assert r.status_code == 200, r.text

            async def signup(extra=None):
                name = "l2_" + uuid4().hex[:12]
                body = {
                    "name": name,
                    "display_name": name,
                    "email": name + "@example.invalid",
                    "password": "synthetic password",
                    "terms_version": "2026-09-r2",
                    "age_confirmed": True,
                } | (extra or {})
                r = await c.post("/auth/users", json=body)
                assert r.status_code == 200, r.text
                await durable(r.json())
                return r.json(), body

            async def flow(remote, headers=None, purpose="ordinary"):
                path = "/auth/moderation/access/oauth/begin" if purpose == "moderation" else "/auth/oauth/authorize"
                body = {
                    "provider" if purpose == "moderation" else "provider_id": "test",
                    "redirect_uri": "http://127.0.0.1:55909/oauth/callback",
                }
                r = await c.post(path, json=body, headers=headers)
                assert r.status_code == 200, r.text
                r = r.json()
                q = parse_qs(urlparse(r["authorize_url"]).query)
                assert len(r["state"]) == 64 and q["code_challenge_method"] == ["S256"]
                auth = await c.post(
                    r["authorize_url"], data={"id": remote, "name": "Synthetic OAuth"}, follow_redirects=False
                )
                assert auth.is_redirect, auth.text
                callback = parse_qs(urlparse(auth.headers["location"]).query)
                assert callback["state"] == [r["state"]]
                return {"state": r["state"], "code": callback["code"][0]}

            first, _ = await signup()
            uid = UUID(first["user"]["id"])
            ordinary = {"Authorization": "Bearer " + first["access_token"]}
            remote = uuid4().hex
            linked = await c.post("/auth/oauth/links/me", headers=ordinary, json=await flow(remote, ordinary))
            assert linked.status_code == 200, linked.text
            oauth = await c.post("/auth/sessions/oauth", json=await flow(remote))
            assert oauth.status_code == 200, oauth.text
            await durable(oauth.json()["login"])
            refresh = await c.put("/auth/session", json={"refresh_token": first["refresh_token"]})
            assert refresh.status_code == 200, refresh.text
            await durable(refresh.json())
            assert (await c.put("/auth/session", json={"refresh_token": first["refresh_token"]})).status_code == 401
            admin = await c.post(
                "/auth/sessions",
                json={"name_or_email": "admin2", "password": "secure admin2 password", "mfa_code": f.totp()},
            )
            assert admin.status_code == 200, admin.text
            staff = {"Authorization": "Bearer " + admin.json()["access_token"]}
            impersonated = await c.post("/auth/sessions/" + str(uid), headers=staff)
            assert impersonated.status_code == 200, impersonated.text
            await durable(impersonated.json())
            print(
                "PASS HTTP password signup, existing-link OAuth, refresh rotation and impersonation all produce usable durable ordinary sessions",
                flush=True,
            )
            remote2 = uuid4().hex
            count = await db.fetchval("SELECT count(*) FROM users")
            registered = await c.post("/auth/sessions/oauth", json=await flow(remote2))
            assert registered.status_code == 200, registered.text
            assert await db.fetchval("SELECT count(*) FROM users") == count
            second, body = await signup({"oauth_register_token": registered.json()["register_token"]})
            assert await db.fetchval("SELECT count(*) FROM users") == count + 1
            assert await db.fetchval(
                "SELECT user_id FROM oauth2_links WHERE provider_id='test' AND remote_user_id=$1", remote2
            ) == UUID(second["user"]["id"])
            body["name"] = "replay_" + uuid4().hex[:8]
            body["email"] = body["name"] + "@example.invalid"
            assert (await c.post("/auth/users", json=body)).status_code != 200
            print(
                "PASS unknown ordinary OAuth yields registration capability only; explicit HTTP signup atomically creates one user/link/durable session; registration replay denied",
                flush=True,
            )
            case = str(uuid4())
            r = await c.post(
                "/auth/moderation/admin/open",
                headers=staff,
                json={
                    "id": case,
                    "target_id": str(uid),
                    "source": "own_review",
                    "private_evidence": {"facts": "Synthetic OAuth boundary"},
                },
            )
            assert r.status_code == 200, r.text
            command = {
                "case_id": case,
                "request_key": str(uuid4()),
                "expected_revision": 0,
                "outcome": "restrict",
                "rationale": "Synthetic account restriction",
                "ground": "Synthetic independent security ground",
                "rule_version": "Synthetic only",
                "automation": "Synthetic human command",
                "scope": "Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten",
                "redress": "At least six calendar months human review",
                "misconduct_facts": "Synthetic actual security evidence",
                "proportionality": "Synthetic minimum appropriate measure",
                "hearing": "Synthetic urgent grounds assessed",
            }
            applicable = command | {
                "article23_applicability": "applies",
                "article23_basis": "Synthetic evidenced applicability of the misuse ground",
            }
            assert (await c.post("/auth/moderation/admin/decide", headers=staff, json=applicable)).status_code == 409
            applicable |= {
                "prior_warning": "Synthetic actual prior warning",
                "absolute_frequency": "Synthetic 12 instances",
                "relative_frequency": "Synthetic 12 of 14 instances",
                "seriousness": "Synthetic individually assessed seriousness",
                "intent_assessment": "Synthetic actual intention assessment",
            }
            assert (await c.post("/auth/moderation/admin/decide", headers=staff, json=applicable)).status_code == 409
            from datetime import datetime, timedelta, timezone

            applicable["ends_at"] = (datetime.now(timezone.utc) + timedelta(days=3)).isoformat()
            r = await c.post("/auth/moderation/admin/decide", headers=staff, json=applicable)
            assert r.status_code == 200, r.text
            restored = command | {"request_key": str(uuid4()), "expected_revision": 1, "outcome": "restore"}
            r = await c.post("/auth/moderation/admin/decide", headers=staff, json=restored)
            assert r.status_code == 200, r.text
            command |= {"request_key": str(uuid4()), "expected_revision": 2}
            r = await c.post("/auth/moderation/admin/decide", headers=staff, json=command)
            assert r.status_code == 200, r.text
            print(
                "PASS actual account Article23 applicable omission/permanent bypass denied; complete finite assessment accepted; independent security-ground restriction remains available",
                flush=True,
            )

            async def snapshot():
                return await db.fetchval(
                    "SELECT jsonb_build_object('users',(SELECT jsonb_agg(to_jsonb(u)) FROM users u),'sessions',(SELECT jsonb_agg(to_jsonb(s)) FROM sessions s),'refresh',(SELECT jsonb_agg(to_jsonb(r)) FROM session_refresh_tokens r),'links',(SELECT jsonb_agg(to_jsonb(l)) FROM oauth2_links l))::text"
                )

            before = await snapshot()
            callback = await flow(remote, purpose="moderation")
            proof = await c.post("/auth/moderation/access/oauth/finish", json=callback)
            assert proof.status_code == 200, proof.text
            assert await snapshot() == before
            cap = {"x-moderation-capability": proof.json()["capability"]}
            assert (await c.get("/auth/moderation/inbox", headers=cap)).status_code == 200
            assert (
                await c.get("/auth/session", headers={"Authorization": "Bearer " + proof.json()["capability"]})
            ).status_code == 401
            assert (await c.post("/auth/moderation/access/oauth/finish", json=callback)).status_code == 401
            assert (await c.post("/auth/sessions/oauth", json=await flow(remote))).status_code != 200
            for dest, purpose in [
                ("/auth/sessions/oauth", "moderation"),
                ("/auth/moderation/access/oauth/finish", "ordinary"),
            ]:
                r = await c.post(dest, json=await flow(remote, purpose=purpose))
                assert r.status_code == 401, r.text
            for unknown in [remote2 + "missing", uuid4().hex]:
                r = await c.post("/auth/moderation/access/oauth/finish", json=await flow(unknown, purpose="moderation"))
                assert r.status_code == 401, r.text
            assert await snapshot() == before
            print(
                "PASS disabled linked OAuth dedicated proof preserves users/last_login/sessions/refresh/links exactly; ordinary/purpose crossing/replay/unknown provider recipient denied without registration",
                flush=True,
            )
    finally:
        if db:
            await db.close()
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
