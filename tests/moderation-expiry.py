"""Actual finite account/content access with both expiry workers disabled.
Only owned synthetic databases; original functions restored in finally.
"""

import asyncio, importlib.util, json, os, subprocess, threading, time
from datetime import datetime, timedelta, timezone
from pathlib import Path
from uuid import UUID, uuid4
import asyncpg, httpx, jwt

spec = importlib.util.spec_from_file_location("consumers", Path(__file__).with_name("moderation-consumers.py"))
x = importlib.util.module_from_spec(spec)
spec.loader.exec_module(x)
f = x.f


async def main():
    db = backend = None
    saved = []
    effect_original = None
    server = x.ThreadingHTTPServer(("127.0.0.1", 55907), x.Sandbox)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    smtp = f.SMTP(("127.0.0.1", 55903), f.SMTPHandler)
    threading.Thread(target=smtp.serve_forever, daemon=True).start()
    try:
        for name in ["l2backend", "l2challenges"]:
            connection = await asyncpg.connect("postgresql://l2test@127.0.0.1:55900/" + name)
            original = await connection.fetchval("SELECT pg_get_functiondef('moderation_maintenance()'::regprocedure)")
            saved.append((connection, original))
            await connection.execute(
                "CREATE OR REPLACE FUNCTION moderation_maintenance() RETURNS integer LANGUAGE sql AS $$ SELECT 0 $$"
            )
        backend, db = [v[0] for v in saved]
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
            "expiry-valkey.log",
        )
        await f.wait_port(55902)
        f.start([str(f.ROOT / "target/debug/academy"), "serve"], "expiry-backend.log")
        await f.wait_port(55901)
        proc = subprocess.Popen(
            [str(x.WORK / "challenges-ms/target/debug/challenges")],
            cwd=x.WORK / "challenges-ms",
            env=os.environ | {"CONFIG_PATH": str(f.BASE / "challenges.toml"), "RUST_LOG": "warn"},
            stdout=(f.BASE / "expiry-challenges.log").open("w"),
            stderr=subprocess.STDOUT,
        )
        f.CHILDREN.append(proc)
        await f.wait_port(55904)
        async with httpx.AsyncClient(timeout=20) as c:
            base = "http://127.0.0.1:55901"
            ch = "http://127.0.0.1:55904"

            async def login(name, password, **extra):
                r = await c.post(base + "/auth/sessions", json={"name_or_email": name, "password": password, **extra})
                assert r.status_code == 200, r.text
                return r.json()

            admin = await login("admin2", "secure admin2 password", mfa_code=f.totp())
            foo = await login("foo", "foo password")
            staff = {"Authorization": "Bearer " + admin["access_token"]}
            uid = foo["user"]["id"]
            old = {"Authorization": "Bearer " + foo["access_token"]}
            proof = await c.post(
                base + "/auth/moderation/access/password", json={"name_or_email": "foo", "password": "foo password"}
            )
            assert proof.status_code == 200, proof.text
            rights = {"x-moderation-capability": proof.json()["capability"]}

            async def open_case(kind, target, authority=False):
                case = str(uuid4())
                body = {
                    "id": case,
                    "target_id": target,
                    "source": "authority_order" if authority else "own_review",
                    "private_evidence": {
                        "facts": "Synthetic finite restriction",
                        "authority": "Synthetic order authority",
                        "order_reference": "Synthetic order",
                        "notification_instructions": "Synthetic immediate notice",
                    },
                }
                if kind == "account":
                    url = base + "/auth/moderation/admin/open"
                else:
                    url = ch + "/moderation/cases"
                    body["target_kind"] = kind
                r = await c.post(url, headers=staff, json=body)
                assert r.status_code == 200, r.text
                return case

            async def decide(kind, case, outcome, rev=0, seconds=None):
                scope = {
                    "account": "Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten",
                    "subtask": "Diese Teilaufgabe auf Bootstrap Academy",
                }[kind]
                body = {
                    "request_key": str(uuid4()),
                    "case_id": case,
                    "expected_revision": rev,
                    "outcome": outcome,
                    "rationale": "Synthetic explicit finite measure",
                    "ground": "Synthetic independently assessed ground",
                    "rule_version": "Synthetic rules, no historical legal assertion",
                    "automation": "Human synthetic command",
                    "scope": scope,
                    "redress": "At least six calendar months human review",
                    "misconduct_facts": "Synthetic assessed misconduct",
                    "proportionality": "Synthetic individual assessment",
                    "hearing": "Synthetic hearing recorded",
                    "order_event_evidence": "Synthetic actual order lifecycle event",
                }
                if seconds:
                    body["ends_at"] = (datetime.now(timezone.utc) + timedelta(seconds=seconds)).isoformat()
                if kind == "subtask":
                    record = await c.get(ch + "/moderation/cases/" + case, headers=staff)
                    assert record.status_code == 200, record.text
                    body["reviewed_content_revision"] = record.json()["review_target"]["revision"]
                r = await c.post(
                    base + "/auth/moderation/admin/decide" if kind == "account" else ch + "/moderation/decisions",
                    headers=staff,
                    json=body,
                )
                assert r.status_code == 200, r.text
                return r.json()

            case = await open_case("account", uid)
            await decide("account", case, "restrict", seconds=3)
            historical = await backend.fetchval(
                "SELECT public_statement::text FROM moderation_decisions WHERE case_id=$1", UUID(case)
            )
            assert (await c.get(base + "/auth/session", headers=old)).status_code == 401
            assert not await backend.fetchval("SELECT enabled FROM users WHERE id=$1", UUID(uid))
            await asyncio.sleep(3.2)
            fresh = await login("foo", "foo password")
            assert fresh["user"]["enabled"]
            assert not await backend.fetchval(
                "SELECT enabled FROM users WHERE id=$1", UUID(uid)
            ), "fixture worker unexpectedly projected expiry"
            assert await backend.fetchval("SELECT enabled FROM user_composites WHERE id=$1", UUID(uid))
            assert (await c.get(base + "/auth/session", headers=old)).status_code == 401
            assert (
                await c.get(base + "/auth/session", headers={"Authorization": "Bearer " + fresh["access_token"]})
            ).status_code == 200
            inbox = await c.get(base + "/auth/moderation/inbox", headers=rights)
            assert inbox.status_code == 200, inbox.text
            row = next(m for m in inbox.json()["backend"] if m["case_id"] == case)
            assert row["effective"]["enabled"]
            assert (
                await backend.fetchval(
                    "SELECT public_statement::text FROM moderation_decisions WHERE case_id=$1", UUID(case)
                )
                == historical
            )
            assert not await backend.fetchval(
                "SELECT enabled FROM user_composites WHERE name='bar'"
            ), "unknown legacy disable escaped"
            print(
                "PASS no maintenance: finite account restriction expires for fresh real password/session admission and current reason view; materialized flag remains false, old revoked token stays denied, legacy disable and immutable statement remain",
                flush=True,
            )
            unmanaged = uuid4()
            unmanaged_name = "unmanaged_" + unmanaged.hex[:8]
            # Model an imported disabled row created after this migration. The public
            # CLI's unsupported reasonless --disabled option is independently rejected.
            async with backend.transaction():
                await backend.execute(
                    "INSERT INTO users(id,name,email,email_verified,created_at,enabled,admin) VALUES($1,$2,$3,true,clock_timestamp(),false,false)",
                    unmanaged,
                    unmanaged_name,
                    unmanaged_name + "@example.invalid",
                )
                for table in ["user_profiles", "user_invoice_info", "user_passwords"]:
                    await backend.execute(
                        f"INSERT INTO {table} SELECT (jsonb_populate_record(NULL::{table},to_jsonb(row)||jsonb_build_object('user_id',$1::uuid))).* FROM {table} row WHERE user_id=$2",
                        unmanaged,
                        UUID(uid),
                    )
            assert not await backend.fetchval("SELECT enabled FROM user_composites WHERE id=$1", unmanaged)
            opened = await open_case("account", str(unmanaged))
            assert not await backend.fetchval("SELECT enabled FROM user_composites WHERE id=$1", unmanaged)
            assert (
                await c.post(
                    base + "/auth/sessions", json={"name_or_email": unmanaged_name, "password": "foo password"}
                )
            ).status_code != 200
            observed = str(
                await backend.fetchval(
                    "SELECT id FROM moderation_cases WHERE subject=$1 AND source='legacy_import'", unmanaged
                )
            )
            await decide("account", opened, "warn")
            assert not await backend.fetchval("SELECT enabled FROM user_composites WHERE id=$1", unmanaged)
            await decide("account", observed, "restore", 1)
            assert (await login(unmanaged_name, "foo password"))["user"]["enabled"]
            process = await asyncio.create_subprocess_exec(
                str(f.ROOT / "target/debug/academy"),
                "admin",
                "user",
                "create",
                "--disabled",
                "forbidden_disabled",
                "forbidden_disabled@example.invalid",
                "synthetic password",
                cwd=f.ROOT,
                env=f.ENV,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.STDOUT,
            )
            await process.communicate()
            assert process.returncode != 0
            assert not await backend.fetchval("SELECT EXISTS(SELECT 1 FROM users WHERE name='forbidden_disabled')")
            print(
                "PASS post-migration imported disabled account stays denied on case open and unrelated warning; unknown observed hold requires explicit restoration, while reasonless disabled CLI creation is rejected",
                flush=True,
            )
            # Actual account-row lock wait crosses a proposed finite decision end. A
            # stale pre-lock effect must neither delete the live session nor be committed.
            waiting = await open_case("account", uid)
            tx = backend.transaction()
            await tx.start()
            await backend.fetchval("SELECT id FROM users WHERE id=$1 FOR UPDATE", UUID(uid))
            body = {
                "case_id": waiting,
                "request_key": str(uuid4()),
                "expected_revision": 0,
                "outcome": "restrict",
                "ends_at": (datetime.now(timezone.utc) + timedelta(seconds=1)).isoformat(),
                "rationale": "Synthetic blocked finite decision",
                "ground": "Synthetic independent ground",
                "rule_version": "Synthetic only",
                "automation": "Synthetic human",
                "scope": "Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten",
                "redress": "At least six calendar months human review",
                "misconduct_facts": "Synthetic actual misconduct",
                "proportionality": "Synthetic proportionate",
                "hearing": "Synthetic hearing",
            }
            pending = asyncio.create_task(c.post(base + "/auth/moderation/admin/decide", headers=staff, json=body))
            await asyncio.sleep(1.3)
            assert not pending.done()
            await tx.commit()
            result = await pending
            assert result.status_code == 409, result.text
            assert (
                await backend.fetchval("SELECT count(*) FROM moderation_decisions WHERE case_id=$1", UUID(waiting)) == 0
            )
            assert (
                await c.get(base + "/auth/session", headers={"Authorization": "Bearer " + fresh["access_token"]})
            ).status_code == 200
            print(
                "PASS real user-row lock wait crossing proposed end rejects stale restriction without committing a statement or revoking a still-live fresh session",
                flush=True,
            )
            # A second, independent order remains controlling after an ordinary measure ends.
            first = await open_case("account", uid)
            await decide("account", first, "restrict", seconds=3)
            order = await open_case("account", uid, True)
            await decide("account", order, "authority_start")
            await asyncio.sleep(3.2)
            denied = await c.post(base + "/auth/sessions", json={"name_or_email": "foo", "password": "foo password"})
            assert denied.status_code != 200
            assert not await backend.fetchval("SELECT enabled FROM user_composites WHERE id=$1", UUID(uid))
            await decide("account", order, "authority_end", 1)
            fresh = await login("foo", "foo password")
            print(
                "PASS elapsed ordinary account hold cannot defeat another live authority case; supported order end permits fresh admission without reviving old sessions",
                flush=True,
            )
            fixture = (
                (x.WORK / "challenges-ms/challenges/src/services/fixtures/authored_export.sql")
                .read_text()
                .replace("00000000-0000-0000-0000-000000000100", uid)
                .replace("00000000-0000-0000-0000-000000000200", admin["user"]["id"])
            )
            await db.execute(fixture)
            name = "expiry_" + uuid4().hex[:8]
            process = await asyncio.create_subprocess_exec(
                str(f.ROOT / "target/debug/academy"),
                "admin",
                "user",
                "create",
                "--verified",
                name,
                name + "@example.invalid",
                "synthetic password",
                cwd=f.ROOT,
                env=f.ENV,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.STDOUT,
            )
            out = await process.communicate()
            assert process.returncode == 0, out
            viewer = await login(name, "synthetic password")
            vh = {"Authorization": "Bearer " + viewer["access_token"]}
            task = "00000000-0000-0000-0000-000000000003"
            target = "00000000-0000-0000-0000-000000000102"
            path = ch + "/tasks/" + task + "/questions/" + target
            case = await open_case("subtask", target)
            assert (
                await c.get(path, headers=vh)
            ).status_code == 404, "case opening restored originally unpublished content"
            legacy = str(
                await db.fetchval(
                    "SELECT id FROM moderation_cases WHERE target_id=$1 AND source='legacy_import' AND private_evidence->>'observed_effect'='hide'",
                    UUID(target),
                )
            )
            await decide("subtask", legacy, "restore", 1)
            positive = await c.get(path, headers=vh)
            assert positive.status_code == 200, (positive.status_code, positive.text)
            await decide("subtask", case, "provisional", seconds=3)
            historical = await db.fetchval(
                "SELECT public_statement::text FROM moderation_decisions WHERE case_id=$1", UUID(case)
            )
            assert (await c.get(path, headers=vh)).status_code == 404
            await asyncio.sleep(3.2)
            positive = await c.get(path, headers=vh)
            assert positive.status_code == 200, positive.text
            assert not await db.fetchval("SELECT enabled FROM challenges_subtasks WHERE id=$1", UUID(target))
            listed = await c.get(ch + "/subtasks?enabled=true", headers=vh)
            assert listed.status_code == 200 and any(
                r["id"] == target and r["enabled"] for r in listed.json()
            ), listed.text
            ih = {
                "Authorization": "Bearer "
                + jwt.encode(
                    {"aud": "challenges", "exp": int(time.time()) + 900},
                    "synthetic-l2-challenges-internal",
                    algorithm="HS256",
                )
            }
            before = await db.fetchval("SELECT count(*) FROM moderation_targets")
            export = await c.get(ch + "/_internal/users/" + uid + "/export", headers=ih)
            assert export.status_code == 200, export.text
            assert next(r for r in export.json()["subtasks_created"] if r["id"] == target)["enabled"]
            assert await db.fetchval("SELECT count(*) FROM moderation_targets") == before
            detail = await c.get(ch + "/moderation/cases/" + case, headers=staff)
            assert detail.json()["effective"]["enabled"]
            assert (
                await db.fetchval(
                    "SELECT public_statement::text FROM moderation_decisions WHERE case_id=$1", UUID(case)
                )
                == historical
            )
            print(
                "PASS no maintenance: actual nonauthor question admission, filtered list, admin effect and T11 read-only export agree at expiry while stored flag/history remain unchanged",
                flush=True,
            )
            # Fence the final projection in PostgreSQL, after candidate selection, to
            # deterministically exercise both an expiry and a committed independent hold.
            effect_original = await db.fetchval(
                "SELECT pg_get_functiondef('moderation_effect(text,uuid)'::regprocedure)"
            )
            await db.execute(
                effect_original.replace("public.moderation_effect(", "public.moderation_effect_fixture_original(", 1)
            )
            await db.execute(
                "CREATE OR REPLACE FUNCTION moderation_effect(p_kind text,p_id uuid) RETURNS jsonb LANGUAGE plpgsql VOLATILE AS $$ BEGIN PERFORM pg_advisory_xact_lock(55900123); RETURN moderation_effect_fixture_original(p_kind,p_id); END $$"
            )
            crossing = await open_case("subtask", target)
            await decide("subtask", crossing, "provisional", seconds=2)
            tx = db.transaction()
            await tx.start()
            await db.execute("SELECT pg_advisory_xact_lock(55900123)")
            pending = asyncio.create_task(c.get(ch + "/subtasks?enabled=true", headers=vh))
            await asyncio.sleep(2.2)
            assert not pending.done()
            await tx.commit()
            response = await pending
            assert response.status_code == 200 and any(
                r["id"] == target and r["enabled"] for r in response.json()
            ), response.text
            hiding = await open_case("subtask", target)
            row = (await c.get(ch + "/moderation/cases/" + hiding, headers=staff)).json()
            tx = db.transaction()
            await tx.start()
            await db.execute("SELECT pg_advisory_xact_lock(55900123)")
            pending = asyncio.create_task(c.get(ch + "/tasks/" + task + "/questions", headers=vh))
            await asyncio.sleep(0.2)
            assert not pending.done()
            command = {
                "case_id": hiding,
                "request_key": str(uuid4()),
                "expected_revision": 0,
                "reviewed_content_revision": row["review_target"]["revision"],
                "outcome": "remove",
                "rationale": "Synthetic independent removal after list selection",
                "ground": "Synthetic confirmed separate ground",
                "rule_version": "Synthetic immutable rule",
                "automation": "Synthetic human",
                "scope": "Diese Teilaufgabe auf Bootstrap Academy",
                "redress": "At least six calendar months human review",
            }
            await db.fetchval("SELECT moderation_decide($1,$2::jsonb)", UUID(admin["user"]["id"]), json.dumps(command))
            await tx.commit()
            response = await pending
            assert response.status_code == 200 and not any(r["id"] == target for r in response.json()), response.text
            filtered = await c.get(ch + "/subtasks?enabled=false", headers=staff)
            assert filtered.status_code == 200 and all(not r["enabled"] for r in filtered.json())
            await db.execute(effect_original)
            await db.execute("DROP FUNCTION moderation_effect_fixture_original(text,uuid)")
            effect_original = None
            await decide("subtask", hiding, "restore", 1)
            print(
                "PASS actual list/projection races: candidate hidden before expiry becomes returned enabled; independent removal committed before final projection excludes nonauthor content; requested flags match returned membership",
                flush=True,
            )
            order = await open_case("subtask", target, True)
            await decide("subtask", order, "authority_start")
            assert (await c.get(path, headers=vh)).status_code == 404
            # Actual author withdrawal preserves order/history and cannot be republished by its later end.
            owner = {"Authorization": "Bearer " + fresh["access_token"]}
            withdraw = await c.delete(ch + "/tasks/" + task + "/subtasks/" + target, headers=owner)
            assert withdraw.status_code == 200, withdraw.text
            await decide("subtask", order, "authority_end", 1)
            assert (await c.get(path, headers=vh)).status_code == 404
            state = json.loads(await db.fetchval("SELECT moderation_effect('subtask',$1)::text", UUID(target)))
            assert state["withdrawn"] and not state["enabled"]
            print(
                "PASS independent content authority hold remains effective; actual author withdrawal then order end never republishes deleted content",
                flush=True,
            )
    finally:
        for proc in reversed(f.CHILDREN):
            if proc.poll() is None:
                proc.terminate()
            try:
                proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
        if effect_original and db:
            await db.execute(effect_original)
            await db.execute("DROP FUNCTION IF EXISTS moderation_effect_fixture_original(text,uuid)")
        for connection, original in saved:
            await connection.execute(original)
            await connection.close()
        smtp.shutdown()
        smtp.server_close()
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    asyncio.run(main())
