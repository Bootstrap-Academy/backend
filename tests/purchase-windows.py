"""R2/R6 real local acceptance, configured deadlines, SMTP and COMMIT faults.

T12_STATE must identify the owned synthetic fixture. All duration values here
are explicit test inputs, never an operational SLA or deployment setting.
"""

from pathlib import Path

source = Path(__file__).with_name("contract-lifecycle.py")
prefix = source.read_text().split("async def main():", 1)[0]
prefix = prefix.replace(
    "fault = faults.get(recipient)",
    'fault = faults.get(recipient)\n                if fault == "delay_ack":\n                    time.sleep(3)',
)
exec(compile(prefix, str(source), "exec"), globals())
config = Path(state["config"])
original_config = config.read_text()


async def main():
    for args in [["migrate", "reset", "--force"], ["migrate", "up"], ["migrate", "demo", "--force"]]:
        subprocess.run(
            [str(ROOT / "target/debug/academy"), *args], env=env, cwd=ROOT, check=True, stdout=subprocess.DEVNULL
        )
    db = await asyncpg.connect("postgresql://morpheus@127.0.0.1:55572/t12backend")
    users = [
        UUID(x)
        for x in [
            "a8d95e0f-71ae-4c49-995e-695b7c93848c",
            "94d0e3ca-bf16-486b-a172-b87f4bcbd039",
            "e3f8a50a-a5a3-444a-9026-77336f716d03",
        ]
    ]
    for i, uid in enumerate(users):
        await db.execute(
            "UPDATE users SET email=$2,email_verified=true WHERE id=$1", uid, f"windows-{i}@example.invalid"
        )
        await db.execute(
            "INSERT INTO coins(user_id,coins,withheld_coins) VALUES($1,100000,0) ON CONFLICT(user_id) DO UPDATE SET coins=100000",
            uid,
        )
    start()
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=40) as client:

        def headers(uid):
            return {"Authorization": "Bearer " + token(uid)}

        async def offer(uid, kind="premium_monthly"):
            r = await client.post("/shop/purchases/offers/" + kind, headers=headers(uid))
            assert r.status_code == 200, r.text
            return r.json()["offer"]

        async def buy(uid, o):
            r = await client.post(
                "/shop/purchases/accept",
                headers=headers(uid),
                json={
                    "order_id": o["id"],
                    "offer_hash": o["hash"],
                    "accepted": True,
                    "early_performance_requested": True,
                },
            )
            assert r.status_code == 200, r.text
            return r.json()

        # Two real quotes existed before either acceptance. Client locks cannot
        # enforce this invariant: the account transaction must serialize it.
        first, second = await offer(users[0]), await offer(users[0], "premium_yearly")
        faults["windows-0@example.invalid"] = "reject"
        outcomes = await asyncio.gather(buy(users[0], first), buy(users[0], second))
        assert sorted(x["state"] for x in outcomes) == ["failed", "paid"], outcomes
        pending = next(x for x in outcomes if x["state"] == "paid")
        assert (
            await db.fetchval(
                "SELECT count(*) FROM transactions WHERE id=ANY($1::uuid[])", [UUID(first["id"]), UUID(second["id"])]
            )
            == 1
        )
        # Missing owner-selected policy disables only prospective issuance; it
        # never invents a default or discards an already accepted order.
        stop()
        config.write_text(
            original_config.split("[purchase.provision_window_seconds]", 1)[0]
            + "[finance]"
            + original_config.split("[finance]", 1)[1]
        )
        start()
        assert (await client.post("/shop/purchases/offers/hearts", headers=headers(users[1]))).status_code != 200
        recovered = await offer(users[0])
        assert recovered == pending["offer"]
        assert (await buy(users[0], recovered))["financial_evidence"] == pending["financial_evidence"]
        print(
            "PASS two pre-opened monthly/yearly quotes produce one paid order/ledger; missing duration disables new orders and preserves exact original recovery",
            flush=True,
        )
        # Two seconds is solely a synthetic configured promise.
        stop()
        config.write_text(original_config.replace("= 3600", "= 2"))
        start()
        late = await offer(users[1])
        assert late["provision_window_seconds"] == 2 and "2 Sekunden" in late["text"]
        faults["windows-1@example.invalid"] = "delay_ack"
        late_state = await buy(users[1], late)
        accepted = datetime.fromisoformat(late_state["accepted_at"].replace("Z", "+00:00"))
        deadline = datetime.fromisoformat(late_state["provision_deadline"].replace("Z", "+00:00"))
        assert deadline - accepted == timedelta(seconds=2)
        assert late_state["state"] == "review" and late_state["fulfillment"] is None
        assert await db.fetchval("SELECT count(*) FROM premium WHERE user_id=$1", users[1]) == 0
        assert await db.fetchval("SELECT count(*) FROM transactions WHERE id=$1", UUID(late["id"])) == 1
        body = await client.get(f"/shop/purchases/{late['id']}/documents/confirmation", headers=headers(users[1]))
        assert str(deadline.year) in body.text and late["declaration"] in body.text
        print(
            "PASS saved bound and exact deadline use binding acceptance; SMTP crossing retains original paid claim without activating late",
            flush=True,
        )
        # Entitlement and immutable fulfillment actually commit after the bound.
        # A pre-COMMIT timestamp must not pass as proof of timely availability.
        await db.execute(
            "CREATE FUNCTION l1_delay_premium() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(3); RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER l1_delay_premium AFTER INSERT ON premium DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION l1_delay_premium()"
        )
        committed = await offer(users[2])
        actual = await buy(users[2], committed)
        await db.execute("DROP TRIGGER l1_delay_premium ON premium; DROP FUNCTION l1_delay_premium()")
        assert actual["state"] == "review" and actual["fulfillment"] is not None, actual
        assert actual["provision_timing"]["committed_before_deadline_proven"] is False
        assert await db.fetchval("SELECT count(*) FROM premium WHERE user_id=$1", users[2]) == 1
        assert await db.fetchval("SELECT count(*) FROM transactions WHERE id=$1", UUID(committed["id"])) == 1
        timing = await client.get(f"/shop/purchases/{committed['id']}/documents/timing", headers=headers(users[2]))
        assert timing.status_code == 200 and "kein genauer" in timing.text
        assert (
            await client.get(f"/shop/purchases/{committed['id']}/documents/timing", headers=headers(users[0]))
        ).status_code == 404
        assert (await buy(users[2], committed))["fulfillment"] == actual["fulfillment"]
        for sql in [
            "UPDATE purchase_offers SET offer=offer-'provision_window_seconds'",
            "UPDATE purchase_provision_observations SET observed_at=clock_timestamp()",
        ]:
            try:
                await db.execute(sql)
            except asyncpg.PostgresError:
                pass
            else:
                raise AssertionError("Immutable evidence could be rewritten")
        print(
            "PASS actual late entitlement COMMIT retained as provided with uncertain timing/review; original debit/period/result survive replay; owner-only separate timing evidence is immutable",
            flush=True,
        )
    await db.close()


try:
    asyncio.run(main())
finally:
    stop()
    config.write_text(original_config)
    cache.terminate()
    cache.wait(timeout=10)
    smtp.shutdown()
    smtp.server_close()
