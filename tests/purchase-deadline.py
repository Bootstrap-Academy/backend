"""L1: late SMTP acknowledgment commits before process death/source fulfillment.

Uses only the opt-in disposable T12 fixture. The account lock holds post-ack
processing so the durable deadline disposition is observed at the exact gap.
"""

from pathlib import Path
import threading

smtp_started = threading.Event()
smtp_release = threading.Event()
source = Path(__file__).with_name("contract-lifecycle.py")
prefix = source.read_text().split("async def main():", 1)[0]
prefix = prefix.replace(
    "fault = faults.get(recipient)",
    'fault = faults.get(recipient)\n                if fault == "held_ack":\n                    smtp_started.set()\n                    assert smtp_release.wait(10)',
)
exec(compile(prefix, str(source), "exec"), globals())


async def main():
    for args in [["migrate", "reset", "--force"], ["migrate", "up"], ["migrate", "demo", "--force"]]:
        subprocess.run(
            [str(ROOT / "target/debug/academy"), *args], env=env, cwd=ROOT, check=True, stdout=subprocess.DEVNULL
        )
    db = await asyncpg.connect("postgresql://morpheus@127.0.0.1:55572/t12backend")
    guard = await asyncpg.connect("postgresql://morpheus@127.0.0.1:55572/t12backend")
    user = UUID("a8d95e0f-71ae-4c49-995e-695b7c93848c")
    await db.execute("UPDATE users SET email='deadline@example.invalid',email_verified=true WHERE id=$1", user)
    internal = {
        "Authorization": "Bearer "
        + subprocess.check_output(
            [str(ROOT / "target/debug/academy"), "jwt", "sign", json.dumps({"aud": "shop"})], env=env, text=True
        ).strip()
    }
    headers = {"Authorization": "Bearer " + token(user)}
    start()
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=30) as client:
        cutoff = datetime.now(timezone.utc) + timedelta(seconds=2)
        product = {
            "kind": "webinar",
            "reference": str(uuid4()),
            "title": "Synthetic deadline",
            "description": "Local test",
            "coins": 0,
            "facts": {},
            "revision": "test",
            "service_starts_at": cutoff.isoformat(),
        }
        response = await client.post(f"/shop/_internal/purchase-offers/events/{user}", headers=internal, json=product)
        assert response.status_code == 200, response.text
        offer = response.json()["offer"]
        oid = UUID(offer["id"])
        faults["deadline@example.invalid"] = "held_ack"
        pending = asyncio.create_task(
            client.post(
                f"/shop/_internal/purchases/events/{user}",
                headers=internal,
                json={
                    "order_id": str(oid),
                    "offer_hash": offer["hash"],
                    "accepted": True,
                    "early_performance_requested": False,
                },
            )
        )
        assert await asyncio.to_thread(smtp_started.wait, 10)
        await guard.execute("BEGIN")
        await guard.fetchrow("SELECT id FROM users WHERE id=$1 FOR UPDATE", user)
        await asyncio.sleep(max(0, (cutoff - datetime.now(timezone.utc)).total_seconds()) + 0.1)
        smtp_release.set()
        for _ in range(200):
            row = await db.fetchrow("SELECT state,smtp_accepted_at FROM purchase_progress WHERE order_id=$1", oid)
            if row["smtp_accepted_at"] is not None:
                break
            await asyncio.sleep(0.01)
        assert row["smtp_accepted_at"] >= cutoff and row["state"] == "review", row
        # Post-ack processing is still blocked by our account lock: kill at the gap.
        for _ in range(200):
            blocked = await db.fetchval(
                "SELECT count(*) FROM pg_stat_activity WHERE datname='t12backend' AND wait_event_type='Lock' AND query LIKE 'SELECT id FROM users WHERE id=$1 FOR UPDATE%'"
            )
            if blocked:
                break
            await asyncio.sleep(0.01)
        assert blocked
        server.kill()
        server.wait(timeout=10)
        await asyncio.gather(pending, return_exceptions=True)
        await guard.execute("ROLLBACK")
        start()
        result = await client.get(f"/shop/purchases/{oid}", headers=headers)
        assert result.status_code == 200, result.text
        current = result.json()
        assert current["state"] == "review" and current["fulfillment"] is None
        assert current["offer"]["product"]["service_starts_at"] == offer["product"]["service_starts_at"]
        assert current["financial_evidence"] == {"charged_coins": 0, "ledger_id": None, "no_charge": True}
        assert (
            await db.fetchval(
                "SELECT count(*) FROM purchase_delivery_attempts WHERE order_id=$1 AND observation='smtp_accepted'", oid
            )
            == 1
        )
        assert await db.fetchval("SELECT count(*) FROM purchase_fulfillments WHERE order_id=$1", oid) == 0
        print(
            "PASS late SMTP acknowledgment + deadline review commit atomically; real death before blocked processing and restart preserve original cutoff and unperformed claim",
            flush=True,
        )
    await guard.close()
    await db.close()


try:
    asyncio.run(main())
finally:
    smtp_release.set()
    stop()
    cache.terminate()
    cache.wait(timeout=10)
    smtp.shutdown()
    smtp.server_close()
