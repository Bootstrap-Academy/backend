"""L1/T12 receipt during held purchase and staff resolution before activation."""

from pathlib import Path

SOURCE = Path(__file__).with_name("contract-review-races.py")
exec(compile(SOURCE.read_text().split("\nasync def main():")[0], str(SOURCE), "exec"), globals())


async def main():
    db, pid, aid, until, email = await fixture()
    await db.execute(
        "INSERT INTO coins(user_id,coins,withheld_coins) VALUES($1,100000,0) ON CONFLICT(user_id) DO UPDATE SET coins=100000",
        foo,
    )
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=45) as c:
        headers = {"Authorization": "Bearer " + token(foo)}
        faults[email] = "reject"
        response = await purchase(c, headers)
        assert response.status_code == 200, response.text
        order = response.json()
        oid = UUID(order["offer"]["id"])
        assert order["state"] == "paid" and order["confirmation_smtp_accepted_at"] is None
        assert await db.fetchval("SELECT until FROM premium WHERE id=$1", pid) == until
        declaration = body(email, aid)
        receipt = await c.post("/contracts/cancellations", json=declaration)
        assert receipt.status_code == 200, receipt.text
        did = UUID(declaration["request_key"]["id"])
        observation = await db.fetchval(
            "SELECT evidence::text FROM contract_purchase_observations WHERE declaration_id=$1 AND order_id=$2",
            did,
            oid,
        )
        assert observation and str(oid) in observation
        prior = dict(await db.fetchrow("SELECT * FROM contract_declarations WHERE id=$1", did))
        explicit_end = until - timedelta(days=1)
        resolution = await external(c, did, explicit_end)
        assert resolution.status_code == 200, resolution.text
        resolved = dict(await db.fetchrow("SELECT * FROM contract_declarations WHERE id=$1", did))
        faults.clear()
        await db.execute(
            "UPDATE purchase_progress SET next_attempt_at=clock_timestamp()-interval '1 second' WHERE order_id=$1", oid
        )
        stop()
        start()
        for _ in range(100):
            current = (await c.get("/shop/purchases/" + str(oid), headers=headers)).json()
            if current["state"] == "fulfilled":
                break
            await asyncio.sleep(0.1)
        assert current["state"] == "fulfilled", current
        final = dict(await db.fetchrow("SELECT * FROM contract_declarations WHERE id=$1", did))
        assert final["effective_end"] == resolved["effective_end"] == explicit_end
        assert final["processing_note"] == resolved["processing_note"]
        assert final["processed_at"] == resolved["processed_at"]
        assert await db.fetchval("SELECT until FROM premium WHERE id=$1", pid) > until
        assert await db.fetchval("SELECT count(*) FROM transactions WHERE id=$1", oid) == 1
        evidence = await db.fetchval(
            "SELECT evidence::text FROM contract_premium_operations WHERE declaration_id=$1 ORDER BY operation_id DESC LIMIT 1",
            did,
        )
        assert evidence and str(oid) in evidence
        statement = await c.get(f"/shop/purchases/{oid}/documents/fulfillment", headers=headers)
        assert statement.status_code == 200 and current["fulfillment"]["purchased_until"] in statement.text
        assert (
            await c.get(
                f"/shop/purchases/{oid}/documents/fulfillment", headers={"Authorization": "Bearer " + token(admin)}
            )
        ).status_code == 404
        results["pending_receipt_staff_then_activation"] = {
            "order": str(oid),
            "declaration": str(did),
            "initial_paid_end": until,
            "staff_end": explicit_end,
            "actual_end": current["fulfillment"]["purchased_until"],
            "purchase_observation": json.loads(observation),
            "later_period_evidence": json.loads(evidence),
            "statement": statement.text,
        }
        print(
            "PASS pending paid purchase receipt; staff resolution survives later full-duration activation with original debit and immutable statement",
            flush=True,
        )
    await db.close()


try:
    asyncio.run(main())
finally:
    stop()
    cache.terminate()
    cache.wait(timeout=10)
    smtp.shutdown()
    smtp.server_close()
    (EVID / "pending-cancellation.json").write_text(json.dumps(results, default=str, indent=2))
