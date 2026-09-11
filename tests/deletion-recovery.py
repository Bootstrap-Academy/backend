"""Opt-in local-only HTTP/cache/process failure injection. Requires T10_STATE from the isolated setup."""

import asyncio
import json
import os
from pathlib import Path
import socket
import subprocess
import time
from uuid import UUID

import asyncpg
import httpx
import redis

ROOT = Path(__file__).resolve().parents[1]
state = json.loads(Path(os.environ["T10_STATE"]).read_text())
base = Path(state["base"])
assert base.name.startswith("bootstrap-t10-")
env = os.environ | {"ACADEMY_CONFIG": f'{state["config"]}:{ROOT}/config.dev.toml', "RUST_LOG": "warn"}
server = None


def start():
    global server
    server = subprocess.Popen(
        [str(ROOT / "target/debug/academy"), "serve"],
        env=env,
        cwd=ROOT,
        stdout=(base / "backend.log").open("a"),
        stderr=subprocess.STDOUT,
    )
    for _ in range(100):
        try:
            with socket.create_connection(("127.0.0.1", 55850), timeout=0.1):
                return
        except OSError:
            time.sleep(0.05)
    raise RuntimeError("local backend did not start")


def stop():
    global server
    if server:
        server.terminate()
        server.wait(timeout=15)
        server = None


def token(uid):
    data = {
        "uid": uid,
        "sid": "11111111-1111-4111-8111-111111111111",
        "rt": "01" * 32,
        "data": {"admin": True, "email_verified": True, "mfa": True},
    }
    return subprocess.check_output(
        [str(ROOT / "target/debug/academy"), "jwt", "sign", json.dumps(data)], env=env, text=True
    ).strip()


async def main():
    db = await asyncpg.connect("postgresql://morpheus@127.0.0.1:55550/t10backend")
    cache = redis.Redis(host="127.0.0.1", port=55851)
    admin = "e3f8a50a-a5a3-444a-9026-77336f716d03"
    foo = "a8d95e0f-71ae-4c49-995e-695b7c93848c"
    bar = "94d0e3ca-bf16-486b-a172-b87f4bcbd039"
    try:
        for args in [["migrate", "reset", "--force"], ["migrate", "up"], ["migrate", "demo", "--force"]]:
            subprocess.run([str(ROOT / "target/debug/academy"), *args], env=env, check=True, stdout=subprocess.DEVNULL)
        (base / "events-ok").unlink(missing_ok=True)
        start()
        if os.getenv("T10_RUN_EVENTS"):
            testenv = env | {
                "T6_BACKEND_URL": "http://127.0.0.1:55850/shop/_internal",
                "T6_BACKEND_DB": "postgresql://morpheus@127.0.0.1:55550/t10backend",
                "T6_EVENTS_DB": "postgresql+asyncpg://morpheus@127.0.0.1:55550/t10events",
            }
            subprocess.run(
                [
                    "/tmp/bootstrap-services-review-311/bin/python",
                    "-m",
                    "pytest",
                    "-q",
                    "tests/services/test_deletion_recovery.py",
                ],
                cwd=ROOT.parent / "events-ms",
                env=testenv,
                check=True,
            )
            stop()
            subprocess.run(
                [str(ROOT / "target/debug/academy"), "migrate", "reset", "--force"],
                env=env,
                check=True,
                stdout=subprocess.DEVNULL,
            )
            subprocess.run(
                [str(ROOT / "target/debug/academy"), "migrate", "up"], env=env, check=True, stdout=subprocess.DEVNULL
            )
            subprocess.run(
                [str(ROOT / "target/debug/academy"), "migrate", "demo", "--force"],
                env=env,
                check=True,
                stdout=subprocess.DEVNULL,
            )
            start()
        headers = {"Authorization": "Bearer " + token(admin)}
        oldfoo = {"Authorization": "Bearer " + token(foo)}
        async with httpx.AsyncClient(base_url="http://127.0.0.1:55850", timeout=20) as client:
            assert (await client.get("/auth/users/me", headers=oldfoo)).status_code == 200
            cache.execute_command("ACL", "SETUSER", "default", "-set")
            response = await client.delete("/auth/users/" + foo, headers=headers)
            assert response.status_code == 200, response.text
            assert not await db.fetchval("SELECT EXISTS(SELECT 1 FROM users WHERE id=$1)", UUID(foo))
            deliveries = [json.loads(line) for line in (base / "deliveries.jsonl").read_text().splitlines()]
            assert {d["service"] for d in deliveries if foo in d["path"]} == {"skills", "challenges", "events"}
            assert all(d["auth"] for d in deliveries)
            cache.execute_command("ACL", "SETUSER", "default", "+set")
            cache.flushdb()  # No cache revocation survives. The deleted JWT is still denied.
            assert (await client.get("/auth/users", headers=oldfoo)).status_code == 401
            print(
                "PASS Redis invalidation denied after commit: fanout still reached all services; stale JWT denied",
                flush=True,
            )
            await db.execute(
                """CREATE FUNCTION t10_ack_fault() RETURNS trigger LANGUAGE plpgsql AS $$
                BEGIN IF OLD.service='skills' THEN RAISE EXCEPTION 'synthetic acknowledgement COMMIT failure'; END IF; RETURN OLD; END $$;
                CREATE CONSTRAINT TRIGGER t10_ack_fault AFTER DELETE ON user_deletion_work
                DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION t10_ack_fault();"""
            )
            await db.execute(
                "INSERT INTO user_deletion_work (user_id,service) VALUES ('ffffffff-ffff-4fff-8fff-ffffffffffff','challenges')"
            )
            failed = subprocess.run(
                [str(ROOT / "target/debug/academy"), "task", "retry-user-deletions"], env=env, stdout=subprocess.DEVNULL
            )
            assert failed.returncode != 0
            assert (
                await db.fetchval(
                    "SELECT count(*) FROM user_deletion_work WHERE user_id='ffffffff-ffff-4fff-8fff-ffffffffffff'"
                )
                == 0
            )
            assert await db.fetchval("SELECT count(*) FROM user_deletion_work WHERE service='skills'") == 1
            assert await db.fetchval("SELECT attempts FROM user_deletion_work WHERE service='skills'") == 1
            await db.execute("DROP TRIGGER t10_ack_fault ON user_deletion_work; DROP FUNCTION t10_ack_fault()")
            # The independently committed claim still defers retry after ack rollback.
            await db.execute(
                "UPDATE user_deletion_work SET next_attempt_at=now()-interval '1 minute' WHERE service='skills'"
            )
            subprocess.run(
                [str(ROOT / "target/debug/academy"), "task", "retry-user-deletions"],
                env=env,
                check=True,
                stdout=subprocess.DEVNULL,
            )
            print(
                "PASS successful remote erasure followed by real deferred acknowledgement COMMIT fault is replayable",
                flush=True,
            )
            rows = await db.fetch("SELECT service,attempts FROM user_deletion_work WHERE user_id=$1", UUID(foo))
            assert [(row["service"], row["attempts"]) for row in rows] == [("events", 1)], rows
            print("PASS partial service acknowledgement retains exactly the failed job", flush=True)
            results = await asyncio.gather(*[client.delete("/auth/users/" + bar, headers=headers) for _ in range(2)])
            assert sorted(r.status_code for r in results) == [200, 404], [r.text for r in results]
            assert await db.fetchval("SELECT count(*) FROM user_deletion_work WHERE user_id=$1", UUID(bar)) == 3
            stop()
            await db.execute("UPDATE user_deletion_work SET next_attempt_at=now()-interval '1 minute'")
            start()
            for _ in range(100):
                if await db.fetchval("SELECT count(*) FROM user_deletion_work WHERE service<>'events'") == 0:
                    break
                await asyncio.sleep(0.05)
            assert await db.fetchval("SELECT count(*) FROM user_deletion_work") == 2
            assert await db.fetchval("SELECT min(attempts) FROM user_deletion_work") >= 1
            print(
                "PASS concurrent deletion has one durable job per service; startup recovers later jobs despite failed earliest job",
                flush=True,
            )
            (base / "events-ok").touch()
            await db.execute("UPDATE user_deletion_work SET next_attempt_at=now()-interval '1 minute'")
            # Separate CLI processes contend for the same durable rows.
            cache.execute_command("ACL", "SETUSER", "default", "-@all", "+acl")
            commands = [
                await asyncio.create_subprocess_exec(
                    str(ROOT / "target/debug/academy"),
                    "task",
                    "retry-user-deletions",
                    env=env,
                    stdout=asyncio.subprocess.DEVNULL,
                )
                for _ in range(2)
            ]
            assert await asyncio.gather(*[c.wait() for c in commands]) == [0, 0]
            cache.execute_command("ACL", "SETUSER", "default", "+@all")
            assert await db.fetchval("SELECT count(*) FROM user_deletion_work") == 0
            print("PASS separate concurrent workers finish only remaining service work after recovery", flush=True)
    finally:
        cache.execute_command("ACL", "SETUSER", "default", "+@all")
        stop()
        await db.close()


asyncio.run(main())
