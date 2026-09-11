"""T12 review fixes: opt-in actual HTTP/PostgreSQL/local SMTP lock-order regression."""

from pathlib import Path
import os

SOURCE = Path(__file__).resolve().with_name("contract-lifecycle.py")
assert os.environ.get("T12_STATE"), "Requires an inspected local synthetic T12_STATE"
# Reuse only the inspected disposable process/SMTP/token helpers, never its test main.
prefix = SOURCE.read_text().split("async def main():")[0]
__file__ = str(SOURCE)
exec(compile(prefix, str(SOURCE), "exec"), globals())
EVID = Path(os.environ["T12_EVIDENCE_DIR"])
EVID.mkdir(exist_ok=True)
results = {}
foo = UUID("a8d95e0f-71ae-4c49-995e-695b7c93848c")
admin = UUID("e3f8a50a-a5a3-444a-9026-77336f716d03")


async def fixture():
    stop()
    for args in [["migrate", "reset", "--force"], ["migrate", "up"], ["migrate", "demo", "--force"]]:
        subprocess.run(
            [str(ROOT / "target/debug/academy"), *args], env=env, cwd=ROOT, check=True, stdout=subprocess.DEVNULL
        )
    db = await asyncpg.connect("postgresql://morpheus@127.0.0.1:55572/t12backend")
    pid, aid = uuid4(), uuid4()
    now = datetime.now(timezone.utc)
    until = now + timedelta(days=10)
    email = await db.fetchval("SELECT email FROM users WHERE id=$1", foo)
    await db.execute(
        "INSERT INTO premium(id,user_id,since,until) VALUES($1,$2,$3,$4)", pid, foo, now - timedelta(days=20), until
    )
    await db.execute(
        "INSERT INTO premium_renewal_agreements(id,user_id,received_at,offer_id,monthly_price,recipient,document,terms_pdf,withdrawal_pdf,paid_period_id,confirmation_deadline) VALUES($1,$2,clock_timestamp(),'synthetic',777,$3,'synthetic agreement',$4,$4,$5,$6)",
        aid,
        foo,
        email,
        b"test",
        pid,
        until,
    )
    await db.execute("INSERT INTO premium_renewal_delivery(agreement_id,sent_at) VALUES($1,clock_timestamp())", aid)
    await db.execute("INSERT INTO premium_subscriptions(user_id,plan,agreement_id) VALUES($1,'monthly',$2)", foo, aid)
    with socket.create_connection(("127.0.0.1", 55873), timeout=2) as local_cache:
        local_cache.sendall(b"*1\r\n$7\r\nFLUSHDB\r\n")
        assert local_cache.recv(1024).startswith(b"+OK")
    start()
    await asyncio.sleep(0.3)
    return db, pid, aid, until, email


def body(email, aid=None, requested=None):
    d = {
        "name": "Independent synthetic declaration",
        "email": email,
        "contract": "PREMIUM",
        "cancellation_type": "ORDINARY",
        "details": "Independent original evidence",
        "request_key": {"id": str(uuid4()), "secret": str(uuid4())},
    }
    if aid:
        d["renewal_agreement_id"] = str(aid)
    if requested:
        d["requested_end"] = requested.isoformat()
    return d


def worker():
    return subprocess.run(
        [str(ROOT / "target/debug/academy"), "task", "retry-contract-confirmations"],
        env=env,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )


async def purchase(c, headers):
    quoted = await c.post("/shop/purchases/offers/premium_monthly", headers=headers)
    assert quoted.status_code == 200, quoted.text
    offer = quoted.json()["offer"]
    return await c.post(
        "/shop/purchases/accept",
        headers=headers,
        json={
            "order_id": offer["id"],
            "offer_hash": offer["hash"],
            "accepted": True,
            "early_performance_requested": True,
        },
    )


async def wait_query(db, pattern, advisory=False):
    for _ in range(200):
        row = await db.fetchrow(
            "SELECT query,wait_event FROM pg_stat_activity WHERE datname='t12backend' AND wait_event_type='Lock' AND query LIKE $1 AND ($2::boolean=false OR wait_event='advisory')",
            pattern,
            advisory,
        )
        if row:
            return dict(row)
        await asyncio.sleep(0.025)
    raise AssertionError(("No expected lock wait", pattern))


async def purchase_order(order):
    db, pid, aid, until, email = await fixture()
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=45) as c:
        await db.execute(
            "INSERT INTO coins(user_id,coins,withheld_coins) VALUES($1,100000,0) ON CONFLICT(user_id) DO UPDATE SET coins=100000",
            foo,
        )
        p = body(email, aid)
        did = UUID(p["request_key"]["id"])
        headers = {"Authorization": "Bearer " + token(foo)}
        if order == "committed_before_receipt":
            buy = await purchase(c, headers)
            assert buy.status_code == 200, buy.text
            receipt = await c.post("/contracts/cancellations", json=p)
        else:
            await db.execute(
                "CREATE FUNCTION fix_pause() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(12012999); RETURN NEW; END $$"
            )
            if order == "receipt_first":
                await db.execute(
                    "CREATE TRIGGER fix_pause BEFORE INSERT ON contract_declarations FOR EACH ROW EXECUTE FUNCTION fix_pause()"
                )
            elif order == "mutation_before_receipt_commit_after":
                await db.execute(
                    "CREATE CONSTRAINT TRIGGER fix_pause AFTER UPDATE ON premium DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fix_pause()"
                )
            else:
                await db.execute(
                    "CREATE TRIGGER fix_pause BEFORE UPDATE ON premium FOR EACH ROW EXECUTE FUNCTION fix_pause()"
                )
            await db.fetchval("SELECT pg_advisory_lock(12012999)")
            if order == "receipt_first":
                first = asyncio.create_task(c.post("/contracts/cancellations", json=p))
            else:
                first = asyncio.create_task(purchase(c, headers))
            await wait_query(db, "%", True)
            if order == "receipt_first":
                second = asyncio.create_task(purchase(c, headers))
            else:
                second = asyncio.create_task(c.post("/contracts/cancellations", json=p))
            await wait_query(db, "%users%FOR UPDATE%")
            await db.fetchval("SELECT pg_advisory_unlock(12012999)")
            a, b = await asyncio.gather(first, second)
            receipt, buy = (a, b) if order == "receipt_first" else (b, a)
            await db.execute(
                "DROP TRIGGER fix_pause ON "
                + ("contract_declarations" if order == "receipt_first" else "premium")
                + "; DROP FUNCTION fix_pause()"
            )
        assert receipt.status_code == buy.status_code == 200, (receipt.text, buy.text)
        row = dict(await db.fetchrow("SELECT * FROM contract_declarations WHERE id=$1", did))
        later = await db.fetchval("SELECT until FROM premium WHERE id=$1", pid)
        assert later > until
        assert row["effective_end"] == (later if order == "committed_before_receipt" else until), row
        if order == "committed_before_receipt":
            assert row["processing_note"] is None, row
        else:
            assert "Prüfung" in row["processing_note"], row
        operations = [
            json.loads(r["evidence"])
            for r in await db.fetch(
                "SELECT evidence::text FROM contract_premium_operations WHERE declaration_id=$1", did
            )
        ]
        ledger = [
            dict(r)
            for r in await db.fetch(
                "SELECT id,coins,created_at,description FROM transactions WHERE user_id=$1 AND description='Premium'",
                foo,
            )
        ]
        assert len(ledger) == 1 and ledger[0]["coins"] == -1000
        if order != "committed_before_receipt":
            assert any(
                any(x["id"] == str(ledger[0]["id"]) for x in op["ledger_entries"]) for op in operations
            ), operations
        assert await db.fetchval("SELECT monthly_price FROM premium_renewal_agreements WHERE id=$1", aid) == 777
        assert await db.fetchval("SELECT count(*) FROM premium_subscriptions WHERE user_id=$1", foo) == 0
        assert await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", foo) == 99000
        lookup = await c.post("/contracts/receipts", json=p["request_key"])
        assert lookup.json()["declaration"] == receipt.json()["declaration"]
        results[order] = {
            "original_end": until,
            "paid_end": later,
            "declaration": row,
            "operations": operations,
            "ledger": ledger,
            "receipt": receipt.json()["declaration"],
        }
    await db.close()


async def pending(c, db, aid, until, email):
    p = body(email, aid, until + timedelta(days=15))
    r = await c.post("/contracts/cancellations", json=p)
    assert r.status_code == 200, r.text
    return UUID(p["request_key"]["id"])


async def off(c):
    r = await c.put("/shop/premium/autopay", headers={"Authorization": "Bearer " + token(foo)}, json={"plan": None})
    assert r.status_code == 200, r.text


async def external(c, did, end, verified=True):
    return await c.patch(
        f"/contracts/declarations/{did}",
        headers={"Authorization": "Bearer " + token(admin, True)},
        json={
            "action": "RECORD_EXTERNAL_RESOLUTION",
            "identity_verified": verified,
            "effective_end": end.isoformat(),
            "note": "Identity and original contract verified. Individual earlier resolution and established-contact communication documented; paid access retained. Earlier automatic communication must be superseded.",
        },
    )


async def run_worker():
    w = await asyncio.to_thread(worker)
    return w


async def message_rows(db, did):
    return [
        dict(r) for r in await db.fetch("SELECT * FROM contract_delivery WHERE declaration_id=$1 ORDER BY kind", did)
    ]


async def external_order(order):
    db, pid, aid, until, email = await fixture()
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=45) as c:
        did = await pending(c, db, aid, until, email)
        end = datetime.now(timezone.utc) + timedelta(days=2)
        if order != "staff_before_off":
            await off(c)
        if order in ("worker_before_staff", "failed_ack_before_staff"):
            stop()
            if order == "failed_ack_before_staff":
                await db.execute(
                    "CREATE FUNCTION fix_reject_ack() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.kind='resolution' AND NEW.accepted_at IS NOT NULL THEN RAISE EXCEPTION 'synthetic acknowledgement COMMIT failure'; END IF; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER fix_reject_ack AFTER UPDATE ON contract_delivery DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fix_reject_ack()"
                )
            w = await run_worker()
            assert (w.returncode != 0) == (order == "failed_ack_before_staff"), w.stdout
            before = await message_rows(db, did)
            if order == "failed_ack_before_staff":
                await db.execute("DROP TRIGGER fix_reject_ack ON contract_delivery; DROP FUNCTION fix_reject_ack()")
            start()
            await asyncio.sleep(0.1)
        else:
            before = await message_rows(db, did)
        invalid = await external(c, did, end, False)
        assert invalid.status_code == 400, invalid.text
        r = await external(c, did, end)
        assert r.status_code == 200, r.text
        assert (await external(c, did, end)).status_code == 409
        if order == "staff_before_off":
            await off(c)
        after_action = dict(await db.fetchrow("SELECT * FROM contract_processing_actions WHERE declaration_id=$1", did))
        stop()
        old_count = len([m for m in mails if m["id"] == f"<contract-{did}-resolution@bootstrap.academy>"])
        # Make superseded retries due to exercise the fence independently of the clock.
        await db.execute(
            "UPDATE contract_delivery SET next_attempt_at=clock_timestamp()-interval '1 second' WHERE declaration_id=$1 AND kind='resolution'",
            did,
        )
        w = await run_worker()
        assert w.returncode == 0, w.stdout
        assert len([m for m in mails if m["id"] == f"<contract-{did}-resolution@bootstrap.academy>"]) == old_count
        after = await message_rows(db, did)
        assert await db.fetchval("SELECT effective_end FROM contract_declarations WHERE id=$1", did) == end
        assert (
            await db.fetchval("SELECT effective_end FROM contract_cancellation_schedule WHERE declaration_id=$1", did)
            == end
        )
        assert await db.fetchval("SELECT until FROM premium WHERE id=$1", pid) == until
        prior = next((m for m in before if m["kind"] == "resolution"), None)
        if prior:
            retained = next(m for m in after if m["kind"] == "resolution")
            assert all(
                retained[k] == prior[k]
                for k in ("body", "subject", "recipient", "created_at", "requested_agreement_id")
            )
            correction = next(m for m in after if m["kind"] == "external_resolution")
            assert correction["accepted_at"] is not None and "ersetzt" in correction["body"]
        else:
            assert not any(m["kind"] in ("resolution", "external_resolution") for m in after)
        results[order] = {
            "end": end,
            "processing_action": after_action,
            "messages_before": before,
            "messages_after": after,
            "invalid_status": invalid.status_code,
        }
    await db.close()


# Hold only a resolution after SMTP DATA, using the same synthetic helper server.
held = threading.Event()
release = threading.Event()
hold_resolution = False
original_handle = Sink.handle
source = SOURCE.read_text().split("class Sink(")[1].split("\n\nclass SMTP")[0]
source = "class HeldSink(" + source
source = source.replace(
    '                if fault == "lost_ack":',
    """                if hold_resolution and "Ergänzende Kündigungsbestätigung" in str(message["Subject"]):
                    held.set()
                    release.wait(40)
                if fault == "lost_ack":""",
)
exec(compile(source, str(SOURCE), "exec"), globals())
Sink.handle = HeldSink.handle


async def sending_race(kill_worker=False):
    global hold_resolution
    db, pid, aid, until, email = await fixture()
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=45) as c:
        did = await pending(c, db, aid, until, email)
        await off(c)
        held.clear()
        release.clear()
        hold_resolution = True
        proc = subprocess.Popen(
            [str(ROOT / "target/debug/academy"), "task", "retry-contract-confirmations"],
            env=env,
            cwd=ROOT,
            stdout=(EVID / "sending-worker.log").open("a"),
            stderr=subprocess.STDOUT,
        )
        try:
            assert await asyncio.to_thread(held.wait, 10), "resolution did not reach controlled SMTP"
            claim = dict(
                await db.fetchrow(
                    "SELECT attempts,last_error,next_attempt_at,accepted_at FROM contract_delivery WHERE declaration_id=$1 AND kind='resolution'",
                    did,
                )
            )
            assert (
                claim["attempts"] == 1
                and claim["last_error"] == "UnacknowledgedAttempt"
                and claim["accepted_at"] is None
            )
            end = datetime.now(timezone.utc) + timedelta(days=2)
            action = asyncio.create_task(external(c, did, end))
            wait = await wait_query(db, "SELECT declaration_id FROM contract_delivery%")
            assert not action.done()
            if kill_worker:
                proc.kill()
                await asyncio.to_thread(proc.wait, 10)
            release.set()
            hold_resolution = False
            r = await action
            assert r.status_code == 200, r.text
            await asyncio.to_thread(proc.wait, 10)
            old_count = len([m for m in mails if m["id"] == f"<contract-{did}-resolution@bootstrap.academy>"])
            stop()
            w = await run_worker()
            assert w.returncode == 0, w.stdout
            assert await db.fetchval("SELECT effective_end FROM contract_declarations WHERE id=$1", did) == end
            assert len([m for m in mails if m["id"] == f"<contract-{did}-resolution@bootstrap.academy>"]) == old_count
            messages = await message_rows(db, did)
            assert next(m for m in messages if m["kind"] == "external_resolution")["accepted_at"] is not None
            results["worker_death" if kill_worker else "sending_concurrent_staff"] = {
                "claim_visible_during_smtp": claim,
                "staff_wait": wait,
                "end": end,
                "messages": messages,
            }
        finally:
            release.set()
            hold_resolution = False
            if proc.poll() is None:
                proc.kill()
                proc.wait(timeout=10)
    await db.close()


async def commit_between_receipt_and_first_snapshot():
    db, pid, aid, until, email = await fixture()
    gate = await asyncpg.connect("postgresql://morpheus@127.0.0.1:55572/t12backend")
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=45) as c:
        await db.execute(
            "INSERT INTO coins(user_id,coins,withheld_coins) VALUES($1,100000,0) ON CONFLICT(user_id) DO UPDATE SET coins=100000",
            foo,
        )
        p = body(email, aid)
        did = UUID(p["request_key"]["id"])
        await gate.fetchval("SELECT pg_advisory_lock(hashtextextended($1,12012))", str(did))
        await db.execute(
            "CREATE FUNCTION receipt_gap_pause_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(12012002); RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER receipt_gap_pause_commit AFTER UPDATE ON premium DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION receipt_gap_pause_commit()"
        )
        await db.fetchval("SELECT pg_advisory_lock(12012002)")
        buy = asyncio.create_task(purchase(c, {"Authorization": "Bearer " + token(foo)}))
        purchase_wait = await wait_query(db, "COMMIT", True)
        before_receipt = await db.fetchval("SELECT clock_timestamp()")
        before_visible_paid = await db.fetchval("SELECT until FROM premium WHERE id=$1", pid)
        before_visible_coins = await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", foo)
        receipt = asyncio.create_task(c.post("/contracts/cancellations", json=p))
        receipt_wait = await wait_query(db, "SELECT pg_advisory_xact_lock(hashtextextended%", True)
        await db.fetchval("SELECT pg_advisory_unlock(12012002)")
        bought = await buy
        assert bought.status_code == 200, bought.text
        commit_observed = await db.fetchval("SELECT clock_timestamp()")
        await gate.fetchval("SELECT pg_advisory_unlock(hashtextextended($1,12012))", str(did))
        received = await receipt
        assert received.status_code == 200, received.text
        row = dict(await db.fetchrow("SELECT * FROM contract_declarations WHERE id=$1", did))
        account = dict(await db.fetchrow("SELECT * FROM contract_account_observation WHERE declaration_id=$1", did))
        history = [
            dict(r)
            for r in await db.fetch(
                "SELECT * FROM contract_period_observation WHERE declaration_id=$1 ORDER BY source", did
            )
        ]
        operations = [
            json.loads(r["evidence"])
            for r in await db.fetch(
                "SELECT evidence::text FROM contract_premium_operations WHERE declaration_id=$1", did
            )
        ]
        global_ops = [
            dict(r)
            for r in await db.fetch(
                "SELECT id,transaction_id::text,transaction_started_at,recorded_at,old_period,new_period,ledger_entries FROM premium_period_changes WHERE premium_id=$1 AND old_period IS NOT NULL",
                pid,
            )
        ]
        later = await db.fetchval("SELECT until FROM premium WHERE id=$1", pid)
        assert (
            before_visible_paid == until and before_visible_coins == 99000
        )  # L1 debit committed before gated activation
        assert before_receipt < row["received_at"] < commit_observed < account["observed_at"]
        assert len(global_ops) == 1 and global_ops[0]["recorded_at"] < row["received_at"]
        assert row["effective_end"] == until and row["processing_note"], row
        assert any(r["until"] == until for r in history), history
        assert any(
            op.get("receipt_ordering") == "commit_order_unknown_at_receipt" and op.get("commit_observed_at")
            for op in operations
        ), operations
        assert await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", foo) == 99000
        results["commit_between_receipt_and_first_snapshot"] = {
            "purchase_http": bought.status_code,
            "receipt_http": received.status_code,
            "purchase_wait": purchase_wait,
            "receipt_wait": receipt_wait,
            "original_paid_end": until,
            "paid_end": later,
            "coins_before_commit": before_visible_coins,
            "coins_after_commit": await db.fetchval("SELECT coins FROM coins WHERE user_id=$1", foo),
            "before_receipt": before_receipt,
            "purchase_commit_observed_by_other_connection": commit_observed,
            "declaration": row,
            "account_observation": account,
            "period_observations": history,
            "declaration_operations": operations,
            "global_operations": global_ops,
            "original_boundary_retained_in_declaration": any(r["until"] == until for r in history),
            "original_boundary_selected": row["effective_end"] == until,
            "overlap_flagged": row["processing_note"] is not None,
        }
        await db.execute("DROP TRIGGER receipt_gap_pause_commit ON premium; DROP FUNCTION receipt_gap_pause_commit()")
    await gate.close()
    await db.close()
    print("PASS commit_between_receipt_and_first_snapshot", flush=True)


async def main():
    await commit_between_receipt_and_first_snapshot()
    for order in ("writer_first", "receipt_first", "mutation_before_receipt_commit_after", "committed_before_receipt"):
        await purchase_order(order)
        print("PASS", order, flush=True)
    for order in ("staff_before_worker", "staff_before_off", "worker_before_staff", "failed_ack_before_staff"):
        await external_order(order)
        print("PASS", order, flush=True)
    await sending_race()
    print("PASS concurrent SMTP/staff", flush=True)
    await sending_race(True)
    print("PASS worker death/staff", flush=True)


try:
    asyncio.run(main())
finally:
    stop()
    cache.terminate()
    cache.wait(timeout=10)
    release.set()
    smtp.shutdown()
    smtp.server_close()
    (EVID / "race-results.json").write_text(json.dumps(results, default=str, indent=2))
    (EVID / "mails.json").write_text(json.dumps(mails, default=str, indent=2))
    for f in ["http.log", "valkey.log"]:
        if (base / f).exists():
            (EVID / ("races-" + f)).write_bytes((base / f).read_bytes())
