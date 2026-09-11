"""Opt-in synthetic PostgreSQL/HTTP/process probes; same isolated T10_STATE as deletion-recovery.py.

The local service fixture must support hold-once/hold-started/release-held markers.
T10_EVIDENCE must be a fresh output directory. No production data or service is used.
"""

import asyncio
from collections import Counter
import json
import os
from pathlib import Path
from uuid import UUID

import asyncpg

ROOT = Path(__file__).resolve().parents[1]
STATE = json.loads(Path(os.environ["T10_STATE"]).read_text())
BASE = Path(STATE["base"])
EVIDENCE = Path(os.environ["T10_EVIDENCE"])
assert BASE.name.startswith("bootstrap-t10-")
ENV = os.environ | {"ACADEMY_CONFIG": f'{STATE["config"]}:{ROOT}/config.dev.toml', "RUST_LOG": "warn"}
HEALTHY = UUID("ffffffff-ffff-4fff-8fff-ffffffffffff")
HELD = UUID("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")


def deliveries():
    return [json.loads(line) for line in (BASE / "deliveries.jsonl").read_text().splitlines()]


async def start(label):
    with (EVIDENCE / f"{label}.log").open("x") as log:
        return await asyncio.create_subprocess_exec(
            str(ROOT / "target/debug/academy"),
            "task",
            "retry-user-deletions",
            env=ENV,
            stdout=log,
            stderr=asyncio.subprocess.STDOUT,
        )


async def run(label):
    return await asyncio.wait_for((await start(label)).wait(), timeout=45)


async def fault(db, enabled):
    if enabled:
        await db.execute(
            """
            CREATE FUNCTION t10_scheduling_ack_fault() RETURNS trigger LANGUAGE plpgsql AS $$
            BEGIN IF OLD.service='skills' THEN RAISE EXCEPTION 'synthetic skills acknowledgement COMMIT failure';
            END IF; RETURN OLD; END $$;
            CREATE CONSTRAINT TRIGGER t10_scheduling_ack_fault AFTER DELETE ON user_deletion_work
            DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION t10_scheduling_ack_fault();
        """
        )
    else:
        await db.execute(
            "DROP TRIGGER t10_scheduling_ack_fault ON user_deletion_work; DROP FUNCTION t10_scheduling_ack_fault()"
        )


async def due(db):
    # Only the controlled synthetic fixture advances leases; preserve queue order.
    await db.execute("UPDATE user_deletion_work SET next_attempt_at=next_attempt_at-interval '2 minutes'")


async def hold():
    for marker in ("hold-started", "release-held"):
        (BASE / marker).unlink(missing_ok=True)
    (BASE / "hold-once").write_text(str(HELD))


async def wait_held():
    for _ in range(200):
        if (BASE / "hold-started").exists():
            return
        await asyncio.sleep(0.025)
    raise AssertionError("worker never reached controlled HTTP endpoint")


async def main():
    db = await asyncpg.connect("postgresql://morpheus@127.0.0.1:55550/t10backend")
    children = []
    try:
        assert await db.fetchval("SELECT count(*) FROM user_deletion_work") == 0
        # Original review reproduction, with immediate fresh processes and no clock edits.
        await fault(db, True)
        await db.executemany(
            "INSERT INTO user_deletion_work(user_id,service,next_attempt_at) VALUES ($1,'skills',now()-interval '2 hours')",
            [(UUID(int=i),) for i in range(1, 101)],
        )
        await db.execute(
            "INSERT INTO user_deletion_work(user_id,service,next_attempt_at) VALUES ($1,'challenges',now()-interval '1 hour')",
            HEALTHY,
        )
        for attempt in range(1, 4):
            before = len(deliveries())
            code = await run(f"original-review-pass-{attempt}")
            sent = deliveries()[before:]
            assert code == (1 if attempt == 1 else 0)
            assert len(sent) == (100 if attempt == 1 else 1 if attempt == 2 else 0)
            assert sum(d["service"] == "challenges" for d in sent) == (1 if attempt == 2 else 0)
            assert await db.fetchval("SELECT count(*) FROM user_deletion_work") == (101 if attempt == 1 else 100)
            assert await db.fetchval("SELECT sum(attempts) FROM user_deletion_work WHERE service='skills'") == 100
            print(
                json.dumps(
                    {
                        "original_fresh_pass": attempt,
                        "exit": code,
                        "requests": len(sent),
                        "healthy_requests": sum(d["service"] == "challenges" for d in sent),
                    }
                ),
                flush=True,
            )
        await fault(db, False)
        await due(db)
        assert await run("original-review-fault-cleared") == 0
        assert await db.fetchval("SELECT count(*) FROM user_deletion_work") == 0
        print(
            "PASS original review's 100+1 fixture across three fresh processes without advancing leases; healthy job completes while fault remains",
            flush=True,
        )

        await fault(db, True)
        await db.executemany(
            "INSERT INTO user_deletion_work(user_id,service,next_attempt_at) VALUES ($1,'skills',now()-interval '2 hours')",
            [(UUID(int=i),) for i in range(1, 101)],
        )
        await db.execute(
            "INSERT INTO user_deletion_work(user_id,service,next_attempt_at) VALUES ($1,'challenges',now()-interval '1 hour')",
            HEALTHY,
        )
        report = []
        for attempt in range(1, 4):
            before = len(deliveries())
            code = await run(f"ack-budget-pass-{attempt}")
            sent = deliveries()[before:]
            row = {
                "fresh_process_pass": attempt,
                "exit": code,
                "remote_requests": len(sent),
                "distinct_bad_jobs": len({d["path"] for d in sent if d["service"] == "skills"}),
                "healthy_requests": sum(d["service"] == "challenges" for d in sent),
                "pending": await db.fetchval("SELECT count(*) FROM user_deletion_work"),
                "bad_attempts_recorded": int(
                    await db.fetchval("SELECT sum(attempts) FROM user_deletion_work WHERE service='skills'")
                ),
            }
            assert code == 1 and len(sent) == 100, row
            assert row["healthy_requests"] == (1 if attempt == 2 else 0), row
            assert row["pending"] == (101 if attempt == 1 else 100), row
            assert row["bad_attempts_recorded"] == (100 if attempt == 1 else 199 if attempt == 2 else 299), row
            report.append(row)
            print(json.dumps(row), flush=True)
            # Model a later scheduled pass after lease expiry, still under the fault.
            await due(db)
        with (EVIDENCE / "ack-budget.json").open("x") as out:
            json.dump(report, out, indent=2)
        await fault(db, False)
        assert await run("ack-budget-fault-cleared") == 0
        assert await db.fetchval("SELECT count(*) FROM user_deletion_work") == 0
        print(
            "PASS 100 rejected acknowledgements cannot starve job 101 across fresh processes, including expired leases; fault removal completes retained work",
            flush=True,
        )

        # A failed pre-delivery scheduling COMMIT must never send HTTP.
        await db.execute("INSERT INTO user_deletion_work(user_id,service) VALUES ($1,'skills')", HELD)
        await db.execute(
            """
            CREATE FUNCTION t10_claim_fault() RETURNS trigger LANGUAGE plpgsql AS $$
            BEGIN RAISE EXCEPTION 'synthetic claim COMMIT failure'; END $$;
            CREATE CONSTRAINT TRIGGER t10_claim_fault AFTER UPDATE ON user_deletion_work
            DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION t10_claim_fault();
        """
        )
        before = len(deliveries())
        assert await run("claim-commit-rejected") == 1
        assert len(deliveries()) == before
        assert await db.fetchval("SELECT attempts FROM user_deletion_work") == 0
        await db.execute("DROP TRIGGER t10_claim_fault ON user_deletion_work; DROP FUNCTION t10_claim_fault()")
        assert await run("claim-commit-recovered") == 0
        assert await db.fetchval("SELECT count(*) FROM user_deletion_work") == 0
        print("PASS rejected claim COMMIT sends zero HTTP; fresh process recovers original work", flush=True)

        # Kill a worker while its first HTTP call is held. The committed lease survives.
        await db.execute("INSERT INTO user_deletion_work(user_id,service) VALUES ($1,'skills')", HELD)
        await hold()
        child = await start("crashed-worker")
        children.append(child)
        await wait_held()
        assert await db.fetchval("SELECT attempts FROM user_deletion_work") == 1
        child.kill()
        await child.wait()
        before = len(deliveries())
        assert await run("crash-before-expiry") == 0
        assert len(deliveries()) == before
        (BASE / "release-held").touch()
        await due(db)
        assert await run("crash-after-expiry") == 0
        assert await db.fetchval("SELECT count(*) FROM user_deletion_work") == 0
        print(
            "PASS killed worker leaves durable lease; restart waits until expiry then replays identical work",
            flush=True,
        )

        # Hold the old owner across a synthetic lease expiry and a real fresh takeover.
        # Its late success must not remove the newer generation whose ack is rejected.
        await asyncio.sleep(0.1)  # Allow the previous fixture HTTP handler to release.
        await db.execute("INSERT INTO user_deletion_work(user_id,service) VALUES ($1,'skills')", HELD)
        await hold()
        child = await start("stale-owner")
        children.append(child)
        await wait_held()
        await due(db)
        await fault(db, True)
        assert await run("replacement-owner") == 1
        newer = dict(await db.fetchrow("SELECT attempts,next_attempt_at,last_error FROM user_deletion_work"))
        assert newer["attempts"] == 2
        (BASE / "release-held").touch()
        assert await asyncio.wait_for(child.wait(), timeout=10) == 0
        assert dict(await db.fetchrow("SELECT attempts,next_attempt_at,last_error FROM user_deletion_work")) == newer
        await fault(db, False)
        await due(db)
        assert await run("stale-owner-recovered") == 0
        assert await db.fetchval("SELECT count(*) FROM user_deletion_work") == 0
        print(
            "PASS old worker's delayed acknowledgement leaves the replacement generation and scheduling intact",
            flush=True,
        )

        await db.executemany(
            "INSERT INTO user_deletion_work(user_id,service) VALUES ($1,'challenges')",
            [(UUID(int=i),) for i in range(1000, 1030)],
        )
        before = len(deliveries())
        workers = [await start(f"concurrent-worker-{i}") for i in range(3)]
        children.extend(workers)
        assert await asyncio.gather(*[c.wait() for c in workers]) == [0, 0, 0]
        counts = Counter(d["path"] for d in deliveries()[before:])
        assert len(counts) == 30 and set(counts.values()) == {1}, counts
        assert await db.fetchval("SELECT count(*) FROM user_deletion_work") == 0
        print("PASS three concurrent fresh workers claim 30 jobs once each; queue empty", flush=True)
    finally:
        (BASE / "release-held").touch()
        for child in children:
            if child.returncode is None:
                child.kill()
                await child.wait()
        await db.close()


asyncio.run(main())
