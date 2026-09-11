"""Opt-in disposable PostgreSQL/HTTP checks for T12 committed-period evidence."""

from pathlib import Path
import os

SOURCE = Path(__file__).resolve().with_name("contract-review-races.py")
__file__ = str(SOURCE)
exec(compile(SOURCE.read_text().split("\nasync def main():")[0], str(SOURCE), "exec"), globals())


async def funded():
    data = await fixture()
    await data[0].execute(
        "INSERT INTO coins(user_id,coins,withheld_coins) VALUES($1,100000,0) ON CONFLICT(user_id) DO UPDATE SET coins=100000",
        foo,
    )
    return data


async def evidence(db, did):
    return [
        json.loads(r[0])
        for r in await db.fetch(
            "SELECT evidence::text FROM contract_premium_operations WHERE declaration_id=$1 ORDER BY operation_id", did
        )
    ]


async def unknown_witness(mode):
    db, pid, aid, until, email = await funded()
    pause = {
        "death": "PERFORM pg_advisory_xact_lock(12012003);",
        "timeout": "PERFORM pg_sleep(3);",
        "failure": "RAISE EXCEPTION 'Synthetic witness failure';",
    }[mode]
    await db.execute(
        "CREATE FUNCTION witness_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN "
        + pause
        + " RETURN NEW; END $$; CREATE TRIGGER witness_fault BEFORE INSERT ON premium_period_commit_observation FOR EACH ROW EXECUTE FUNCTION witness_fault()"
    )
    if mode == "death":
        await db.fetchval("SELECT pg_advisory_lock(12012003)")
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=45) as c:
        started = time.monotonic()
        buy = asyncio.create_task(purchase(c, {"Authorization": "Bearer " + token(foo)}))
        if mode == "death":
            await wait_query(db, "INSERT INTO premium_period_commit_observation%", True)
            assert await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", foo) == 99000
            server.kill()
            await asyncio.to_thread(server.wait, 10)
            try:
                await buy
                raise AssertionError("Killed response unexpectedly completed")
            except httpx.TransportError:
                pass
            await db.fetchval("SELECT pg_advisory_unlock(12012003)")
        else:
            bought = await buy
            assert bought.status_code == 200, bought.text
            if mode == "timeout":
                assert 1.8 <= time.monotonic() - started < 8
        assert await db.fetchval("SELECT count(*) FROM premium_period_commit_observation") == 0
        await db.execute(
            "DROP TRIGGER witness_fault ON premium_period_commit_observation; DROP FUNCTION witness_fault()"
        )
        if mode == "death":
            stop()
            start()
        p = body(email, aid)
        r = await c.post("/contracts/cancellations", json=p)
        assert r.status_code == 200, r.text
        did = UUID(p["request_key"]["id"])
        row = dict(await db.fetchrow("SELECT * FROM contract_declarations WHERE id=$1", did))
        ops = await evidence(db, did)
        assert row["effective_end"] == until and "Prüfung" in row["processing_note"], row
        assert any(
            op.get("receipt_ordering") == "commit_order_unknown_at_receipt"
            and op["commit_observed_at"] is None
            and op["ledger_entries"]
            for op in ops
        ), ops
        # Recovery can only record its actual later bound, never retrofit the receipt.
        await db.execute(
            "INSERT INTO premium_period_commit_observation(operation_id) SELECT id FROM premium_period_changes ORDER BY id"
        )
        assert await evidence(db, did) == ops
        assert (
            await db.fetchval("SELECT count(*) FROM transactions WHERE user_id=$1 AND description='Premium'", foo) == 1
        )
        assert await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", foo) == 99000
        assert await db.fetchval("SELECT until FROM premium WHERE id=$1", pid) > until
        await db.execute("DELETE FROM users WHERE id=$1", foo)
        assert await db.fetchval("SELECT count(*) FROM premium_period_changes") == 0
        assert await db.fetchval("SELECT count(*) FROM premium_period_commit_observation") == 0
        assert await evidence(db, did) == ops
        lookup = await c.post("/contracts/receipts", json=p["request_key"])
        assert lookup.json()["declaration"] == r.json()["declaration"]
        results["witness_" + mode] = {
            "declaration": row,
            "operations_after_erasure": ops,
            "purchase_http": 200 if mode != "death" else "response lost on real process death",
            "ledger_count_before_erasure": 1,
        }
    await db.close()
    print("PASS missing/late witness, erasure", mode, flush=True)


async def two_purchases():
    db, pid, aid, until, email = await funded()
    gate = await asyncpg.connect("postgresql://morpheus@127.0.0.1:55572/t12backend")
    p = body(email, aid)
    did = UUID(p["request_key"]["id"])
    await gate.fetchval("SELECT pg_advisory_lock(hashtextextended($1,12012))", str(did))
    await db.execute(
        "CREATE FUNCTION two_pause() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(12012004); RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER two_pause AFTER UPDATE ON premium DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION two_pause()"
    )
    await db.fetchval("SELECT pg_advisory_lock(12012004)")
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=45) as c:
        headers = {"Authorization": "Bearer " + token(foo)}
        first = asyncio.create_task(purchase(c, headers))
        await wait_query(db, "COMMIT", True)
        second = asyncio.create_task(purchase(c, headers))
        await wait_query(db, "%users%FOR UPDATE%")
        receipt = asyncio.create_task(c.post("/contracts/cancellations", json=p))
        await wait_query(db, "SELECT pg_advisory_xact_lock(hashtextextended%", True)
        await db.fetchval("SELECT pg_advisory_unlock(12012004)")
        buys = await asyncio.gather(first, second)
        assert all(r.status_code == 200 for r in buys)
        await gate.fetchval("SELECT pg_advisory_unlock(hashtextextended($1,12012))", str(did))
        r = await receipt
        assert r.status_code == 200, r.text
        ops = await evidence(db, did)
        ledger_ids = {x["id"] for op in ops for x in op["ledger_entries"]}
        row = dict(await db.fetchrow("SELECT * FROM contract_declarations WHERE id=$1", did))
        assert len(ledger_ids) == 2 and row["effective_end"] == until and row["processing_note"], (row, ops)
        assert await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", foo) == 98000
        results["two_queued_purchases"] = {"declaration": row, "operations": ops, "coins": 98000}
    await gate.close()
    await db.close()
    print("PASS two queued real purchases", flush=True)


async def guards_and_historical():
    db, pid, aid, until, email = await funded()
    assert await db.fetchval("SELECT count(*) FROM premium_period_commit_observation") == 0
    txn = db.transaction()
    await txn.start()
    await db.execute("UPDATE premium SET until=until+interval '1 month' WHERE id=$1", pid)
    oid = await db.fetchval("SELECT max(id) FROM premium_period_changes")
    sp = db.transaction()
    await sp.start()
    try:
        await db.execute("INSERT INTO premium_period_commit_observation(operation_id) VALUES($1)", oid)
        raise AssertionError("Same-transaction witness accepted")
    except asyncpg.RaiseError:
        await sp.rollback()
    await txn.commit()
    assert await db.fetchval("SELECT count(*) FROM premium_period_commit_observation") == 0
    before = await db.fetchval("SELECT clock_timestamp()")
    observed = await db.fetchval(
        "INSERT INTO premium_period_commit_observation(operation_id,observed_at) VALUES($1,'2000-01-01Z') RETURNING observed_at",
        oid,
    )
    assert observed >= before
    try:
        await db.execute(
            "UPDATE premium_period_commit_observation SET observed_at='2000-01-01Z' WHERE operation_id=$1", oid
        )
        raise AssertionError("Witness update accepted")
    except asyncpg.RaiseError:
        pass
    # A raw historical operation stays unknown until a true present observation.
    # A supported fresh purchase observes all already committed retained versions.
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=45) as c:
        bought = await purchase(c, {"Authorization": "Bearer " + token(foo)})
        assert bought.status_code == 200
        p = body(email, aid)
        r = await c.post("/contracts/cancellations", json=p)
        did = UUID(p["request_key"]["id"])
        row = dict(await db.fetchrow("SELECT * FROM contract_declarations WHERE id=$1", did))
        assert r.status_code == 200 and row["processing_note"] is None, row
        assert await evidence(db, did) == []
        results["witness_guards_historical"] = {
            "caller_backdated_date_replaced_by": observed,
            "same_transaction_rejected": True,
            "update_rejected": True,
            "declaration": row,
        }
    await db.close()
    print("PASS witness guards and historical catch-up", flush=True)


async def creation_and_background():
    db, pid, aid, until, email = await funded()
    await db.execute("DELETE FROM premium_subscriptions WHERE user_id=$1;", foo)
    await db.execute("DELETE FROM premium WHERE user_id=$1", foo)
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=45) as c:
        r = await purchase(c, {"Authorization": "Bearer " + token(foo)})
        assert r.status_code == 200, r.text
    assert (
        await db.fetchval(
            "SELECT count(*) FROM premium_period_changes p JOIN premium_period_commit_observation w ON w.operation_id=p.id WHERE p.old_period IS NULL AND jsonb_array_length(p.ledger_entries)=1"
        )
        == 1
    )
    await db.close()
    db, pid, aid, until, email = await funded()
    await db.execute("UPDATE premium SET until=clock_timestamp()-interval '1 second' WHERE id=$1", pid)
    stop()
    renewed = await asyncio.to_thread(
        subprocess.run,
        [str(ROOT / "target/debug/academy"), "task", "refresh-premium"],
        env=env,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    assert renewed.returncode == 0, renewed.stdout
    assert await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", foo) == 99223
    ops = [
        dict(r)
        for r in await db.fetch(
            "SELECT p.id,p.old_period,p.new_period,p.ledger_entries,w.observed_at FROM premium_period_changes p JOIN premium_period_commit_observation w ON w.operation_id=p.id WHERE jsonb_array_length(p.ledger_entries)>0"
        )
    ]
    assert len(ops) == 1 and json.loads(ops[0]["ledger_entries"])[0]["coins"] == -777, ops
    results["new_period_and_background_fixed_price"] = {
        "new_period_observed": True,
        "background_operations": ops,
        "coins": 99223,
    }
    await db.close()
    print("PASS new period and background fixed-price renewal", flush=True)


async def unsupported_clock():
    db, pid, aid, until, email = await funded()
    stop()
    config = Path(state["config"])
    original = config.read_text()
    config.write_text(original.replace("@127.0.0.1:55572", "@localhost:55572"))
    try:
        start()
        async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=45) as c:
            bought = await purchase(c, {"Authorization": "Bearer " + token(foo)})
            assert bought.status_code == 200, bought.text
            p = body(email, aid)
            r = await c.post("/contracts/cancellations", json=p)
            did = UUID(p["request_key"]["id"])
            ops = await evidence(db, did)
            row = dict(await db.fetchrow("SELECT * FROM contract_declarations WHERE id=$1", did))
            assert r.status_code == 200 and row["effective_end"] == until and row["processing_note"], row
            assert any(op.get("shared_receipt_clock") is False and op["ledger_entries"] for op in ops), ops
            results["unsupported_clock_conservative"] = {"declaration": row, "operations": ops}
    finally:
        stop()
        config.write_text(original)
    await db.close()
    print("PASS unsupported hostname clock remains conservative", flush=True)


async def main():
    await unknown_witness("failure")
    await unknown_witness("death")
    await unknown_witness("timeout")
    await two_purchases()
    await guards_and_historical()
    await creation_and_background()
    await unsupported_clock()


try:
    asyncio.run(main())
finally:
    stop()
    cache.terminate()
    cache.wait(timeout=10)
    release.set()
    smtp.shutdown()
    smtp.server_close()
    (EVID / "commit-observation-results.json").write_text(json.dumps(results, default=str, indent=2))
    (EVID / "commit-observation-mails.json").write_text(json.dumps(mails, default=str, indent=2))
    for f in ["http.log", "valkey.log"]:
        if (base / f).exists():
            (EVID / ("commit-observation-" + f)).write_bytes((base / f).read_bytes())
