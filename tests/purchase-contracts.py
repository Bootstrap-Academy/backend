"""Opt-in L1 local synthetic HTTP/SMTP/PostgreSQL fault probe (T12_STATE fixture).

Only a newly initialized /tmp/bootstrap-t12-* cluster is accepted. No providers.
"""

from pathlib import Path

# Reuse the committed local-only SMTP/process/token fixture, not its T12 tests.
exec(
    compile(
        Path(__file__).with_name("contract-lifecycle.py").read_text().split("async def main():", 1)[0],
        str(Path(__file__).with_name("contract-lifecycle.py")),
        "exec",
    )
)


async def main():
    for args in [["migrate", "reset", "--force"], ["migrate", "up"], ["migrate", "demo", "--force"]]:
        subprocess.run(
            [str(ROOT / "target/debug/academy"), *args], env=env, cwd=ROOT, check=True, stdout=subprocess.DEVNULL
        )
    db = await asyncpg.connect("postgresql://morpheus@127.0.0.1:55572/t12backend")
    foo = UUID("a8d95e0f-71ae-4c49-995e-695b7c93848c")
    bar = UUID("94d0e3ca-bf16-486b-a172-b87f4bcbd039")
    await db.execute("UPDATE users SET email='l1-foo@example.invalid',email_verified=true WHERE id=$1", foo)
    await db.execute(
        "INSERT INTO coins(user_id,coins,withheld_coins) VALUES($1,100000,0) ON CONFLICT(user_id) DO UPDATE SET coins=100000",
        foo,
    )
    await db.execute("DELETE FROM premium_subscriptions WHERE user_id=$1", foo)
    await db.execute("DELETE FROM premium WHERE user_id=$1", foo)
    await db.execute(
        "INSERT INTO hearts(user_id,hearts,last_refill) VALUES($1,1,clock_timestamp()) ON CONFLICT(user_id) DO UPDATE SET hearts=1,last_refill=clock_timestamp()",
        foo,
    )
    headers = {"Authorization": "Bearer " + token(foo)}
    other = {"Authorization": "Bearer " + token(bar)}
    internal = {
        "Authorization": "Bearer "
        + subprocess.check_output(
            [str(ROOT / "target/debug/academy"), "jwt", "sign", json.dumps({"aud": "shop"})], env=env, text=True
        ).strip()
    }
    start()
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=40) as c:

        async def offer(kind="premium_monthly"):
            r = await c.post("/shop/purchases/offers/" + kind, headers=headers)
            assert r.status_code == 200, r.text
            return r.json()["offer"]

        def acceptance(o):
            return {"order_id": o["id"], "offer_hash": o["hash"], "accepted": True, "early_performance_requested": True}

        async def buy(o):
            r = await c.post("/shop/purchases/accept", headers=headers, json=acceptance(o))
            assert r.status_code == 200, r.text
            return r.json()

        o = await offer()
        a = acceptance(o)
        assert (await c.post("/shop/purchases/accept", headers=other, json=a)).status_code == 404
        assert (
            await c.post("/shop/purchases/accept", headers=headers, json={**a, "offer_hash": "wrong"})
        ).status_code == 409
        assert (
            await c.post("/shop/purchases/accept", headers=headers, json={**a, "early_performance_requested": False})
        ).status_code == 409
        docs = (await c.get(f"/shop/purchases/{o['id']}/documents/terms", headers=headers)).content
        assert docs.startswith(b"%PDF")
        results = await asyncio.gather(*[buy(o) for _ in range(6)])
        assert await db.fetchval("SELECT count(*) FROM transactions WHERE id=$1", UUID(o["id"])) == 1
        assert await db.fetchval("SELECT count(*) FROM purchase_fulfillments WHERE order_id=$1", UUID(o["id"])) == 1
        evidence = json.loads(
            await db.fetchval(
                "SELECT purchase_evidence::text FROM premium_period_changes WHERE purchase_evidence->>'order_id'=$1",
                o["id"],
            )
        )
        assert evidence["original_ledger"]["id"] == o["id"]
        print(
            "PASS exact documents/declarations/owner; six concurrent replays charge and extend once with original ledger evidence",
            flush=True,
        )
        # A full refill rejected without acceptance stays terminal after consumption.
        h = await offer("hearts")
        await db.execute("UPDATE hearts SET hearts=6 WHERE user_id=$1", foo)
        assert (await buy(h))["state"] == "failed"
        await db.execute("UPDATE hearts SET hearts=1 WHERE user_id=$1", foo)
        assert (await buy(h))["state"] == "failed"
        assert await db.fetchval("SELECT count(*) FROM transactions WHERE id=$1", UUID(h["id"])) == 0
        print("PASS terminal no-op remains uncharged after later heart consumption", flush=True)
        # Lost SMTP acknowledgment means no new service, but accepted payment persists.
        h = await offer("hearts")
        faults["l1-foo@example.invalid"] = "lost_ack"
        result = await buy(h)
        assert result["state"] == "paid", result
        assert await db.fetchval("SELECT hearts FROM hearts WHERE user_id=$1", foo) == 1
        assert (
            await db.fetchval(
                "SELECT count(*) FROM purchase_delivery_attempts WHERE order_id=$1 AND observation='handoff_uncertain'",
                UUID(h["id"]),
            )
            == 1
        )
        # Before recovery a free daily refill makes the paid quantity unperformable.
        await db.execute("UPDATE hearts SET hearts=6 WHERE user_id=$1", foo)
        await db.execute(
            "UPDATE purchase_progress SET next_attempt_at=clock_timestamp()-interval '1 minute' WHERE order_id=$1",
            UUID(h["id"]),
        )
        stop()
        start()
        for _ in range(60):
            result = (await c.get("/shop/purchases/" + h["id"], headers=headers)).json()
            if result["state"] == "review":
                break
            await asyncio.sleep(0.1)
        assert result["state"] == "review", result
        assert await db.fetchval("SELECT count(*) FROM purchase_fulfillments WHERE order_id=$1", UUID(h["id"])) == 0
        assert (
            await db.fetchval(
                "SELECT count(*) FROM purchase_delivery_attempts WHERE order_id=$1 AND observation='handoff_uncertain'",
                UUID(h["id"]),
            )
            == 1
        )
        print(
            "PASS SMTP uncertain handoff retained; no hearts before provision; changed refill preserves unperformed paid claim",
            flush=True,
        )
        # Insufficient funds is definitive across top-up and process restart.
        o = await offer()
        await db.execute("UPDATE coins SET coins=0 WHERE user_id=$1", foo)
        assert (await buy(o))["state"] == "failed"
        await db.execute(
            "INSERT INTO coins(user_id,coins,withheld_coins) VALUES($1,100000,0) ON CONFLICT(user_id) DO UPDATE SET coins=100000",
            foo,
        )
        assert (await buy(o))["state"] == "failed"
        assert await db.fetchval("SELECT count(*) FROM transactions WHERE id=$1", UUID(o["id"])) == 0
        print("PASS insufficient-funds rejection cannot charge later top-up", flush=True)
        # Fixed event deadline: a late first successful retry cannot claim service readiness.
        start_at = datetime.now(timezone.utc) + timedelta(seconds=2)
        p = {
            "kind": "webinar",
            "reference": str(uuid4()),
            "title": "Synthetic session",
            "description": "Local test only",
            "coins": 0,
            "facts": {},
            "revision": "test",
            "service_starts_at": start_at.isoformat(),
        }
        r = await c.post(f"/shop/_internal/purchase-offers/events/{foo}", headers=internal, json=p)
        assert r.status_code == 200, r.text
        o = r.json()["offer"]
        a = acceptance(o)
        a["early_performance_requested"] = False
        faults["l1-foo@example.invalid"] = "reject"
        r = await c.post(f"/shop/_internal/purchases/events/{foo}", headers=internal, json=a)
        assert r.status_code == 200, r.text
        assert r.json()["state"] == "paid"
        r = await c.post(
            f"/shop/_internal/purchases/events/{foo}", headers=internal, json={**a, "early_performance_requested": True}
        )
        assert r.status_code == 409, r.text
        await asyncio.sleep(2.1)
        faults.clear()
        await db.execute(
            "UPDATE purchase_progress SET next_attempt_at=clock_timestamp()-interval '1 minute' WHERE order_id=$1",
            UUID(o["id"]),
        )
        stop()
        start()
        for _ in range(60):
            r = await c.get("/shop/purchases/" + o["id"], headers=headers)
            if r.json()["state"] == "review" and r.json()["confirmation_smtp_accepted_at"]:
                break
            await asyncio.sleep(0.1)
        assert r.json()["state"] == "review" and r.json()["confirmation_smtp_accepted_at"], r.text
        assert await db.fetchval("SELECT count(*) FROM transactions WHERE id=$1", UUID(o["id"])) == 0
        print(
            "PASS free acceptance exact replay; late delivery records confirmation and missed-deadline claim without a ledger or fulfillment",
            flush=True,
        )
    # Pre-L1 zero-price builtin configurations remain valid, without fake ledger IDs.
    stop()
    config = Path(state["config"])
    original = config.read_text()
    try:
        config.write_text(original + "\n[premium]\nmonthly_price=0\nyearly_price=0\n[heart]\nrefill_price=0\n")
        faults.clear()
        await db.execute("UPDATE hearts SET hearts=1,last_refill=clock_timestamp() WHERE user_id=$1", foo)
        start()
        async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=40) as c:
            # Configuration changes cannot replace the earlier unresolved paid
            # refill. Preserve it and test zero-price behavior with another user.
            recovered = await offer("hearts")
            assert recovered["product"]["coins"] != 0
            foo = bar
            headers = {"Authorization": "Bearer " + token(foo)}
            await db.execute("UPDATE users SET email='zero-l1@example.invalid',email_verified=true WHERE id=$1", foo)
            await db.execute(
                "INSERT INTO hearts(user_id,hearts,last_refill) VALUES($1,0,clock_timestamp()) ON CONFLICT(user_id) DO UPDATE SET hearts=0,last_refill=clock_timestamp()",
                foo,
            )
            for kind in ["premium_monthly", "hearts"]:
                o = await offer(kind)
                assert o["product"]["coins"] == 0
                result = await buy(o)
                assert result["state"] == "fulfilled", result
                assert result["financial_evidence"] == {"charged_coins": 0, "ledger_id": None, "no_charge": True}
                assert result["fulfillment"]["ledger_id"] is None
                assert await db.fetchval("SELECT count(*) FROM transactions WHERE id=$1", UUID(o["id"])) == 0
                if kind == "premium_monthly":
                    link = json.loads(
                        await db.fetchval(
                            "SELECT purchase_evidence::text FROM premium_period_changes WHERE purchase_evidence->>'order_id'=$1",
                            o["id"],
                        )
                    )
                    assert link["order_id"] == o["id"] and link["original_ledger"] is None
            print(
                "PASS supported zero-price Premium/hearts fulfill with explicit no-charge evidence and complete Premium order link, no invented ledger",
                flush=True,
            )
    finally:
        stop()
        config.write_text(original)
    await db.close()


try:
    asyncio.run(main())
finally:
    stop()
    cache.terminate()
    cache.wait(timeout=10)
    smtp.shutdown()
    smtp.server_close()
