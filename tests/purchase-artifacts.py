"""Reviewer-selected actual contract artifacts/half-heart units and browser fixture."""

from pathlib import Path
import base64, hashlib, http.server, os

EVID = Path(os.environ["L1_EVIDENCE_DIR"])
EVID.mkdir(parents=True, exist_ok=True)
captured_raw = []
source = Path(__file__).with_name("contract-lifecycle.py")
prefix = source.read_text().split("async def main():", 1)[0]
prefix = prefix.replace(
    'message = BytesParser(policy=policy.default).parsebytes(b"".join(lines))',
    'captured_raw.append(b"".join(lines))\n                message = BytesParser(policy=policy.default).parsebytes(b"".join(lines))',
)
exec(compile(prefix, str(source), "exec"), globals())
results = {}


async def main():
    for args in [["migrate", "reset", "--force"], ["migrate", "up"], ["migrate", "demo", "--force"]]:
        subprocess.run(
            [str(ROOT / "target/debug/academy"), *args], env=env, cwd=ROOT, check=True, stdout=subprocess.DEVNULL
        )
    db = await asyncpg.connect("postgresql://morpheus@127.0.0.1:55572/t12backend")
    foo = UUID("a8d95e0f-71ae-4c49-995e-695b7c93848c")
    bar = UUID("94d0e3ca-bf16-486b-a172-b87f4bcbd039")
    address = "l1-fix-foo@example.invalid"
    await db.execute("UPDATE users SET email=$2,email_verified=true WHERE id=$1", foo, address)
    await db.execute(
        "INSERT INTO coins(user_id,coins,withheld_coins) VALUES($1,100000,0) ON CONFLICT(user_id) DO UPDATE SET coins=100000",
        foo,
    )
    await db.execute("DELETE FROM premium_subscriptions WHERE user_id=$1", foo)
    await db.execute("DELETE FROM premium WHERE user_id=$1", foo)
    headers = {"Authorization": "Bearer " + token(foo)}
    other = {"Authorization": "Bearer " + token(bar)}
    start()
    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=45) as c:

        async def offer(kind):
            r = await c.post("/shop/purchases/offers/" + kind, headers=headers)
            assert r.status_code == 200, r.text
            return r.json()["offer"]

        def acceptance(o):
            return {"order_id": o["id"], "offer_hash": o["hash"], "accepted": True, "early_performance_requested": True}

        async def buy(o):
            r = await c.post("/shop/purchases/accept", headers=headers, json=acceptance(o))
            assert r.status_code == 200, r.text
            return r.json()

        faults[address] = "reject"
        p = await offer("premium_monthly")
        pending = await buy(p)
        assert (
            pending["state"] == "paid"
            and pending["fulfillment"] is None
            and pending["confirmation_smtp_accepted_at"] is None
        )
        assert await db.fetchval("SELECT count(*) FROM premium WHERE user_id=$1", foo) == 0
        paths = {}
        for kind in ["terms", "withdrawal", "confirmation"]:
            r = await c.get(f"/shop/purchases/{p['id']}/documents/{kind}", headers=headers)
            assert r.status_code == 200, r.text
            assert (await c.get(f"/shop/purchases/{p['id']}/documents/{kind}", headers=other)).status_code == 404
            ext = "pdf" if kind != "confirmation" else "txt"
            file = EVID / f"actual-premium-{kind}.{ext}"
            file.write_bytes(r.content)
            paths[kind] = {"sha256": hashlib.sha256(r.content).hexdigest(), "bytes": len(r.content)}
        assert (EVID / "actual-premium-terms.pdf").read_bytes() == (
            ROOT / "academy_assets/assets/email/agb-2026-09.pdf"
        ).read_bytes()
        assert (EVID / "actual-premium-withdrawal.pdf").read_bytes() == (
            ROOT / "academy_assets/assets/email/widerrufsbelehrung-2026-09.pdf"
        ).read_bytes()
        confirmation = (EVID / "actual-premium-confirmation.txt").read_text()
        assert p["declaration"] in confirmation and "Mir ist bekannt" not in confirmation
        results["premium_pending"] = {
            "offer": p,
            "status": pending,
            "artifacts": paths,
            "actual_payload": acceptance(p),
        }
        print(
            "PASS actual Premium paid/pending with no access; downloaded PDF bytes equal final embedded assets; neutral declaration only",
            flush=True,
        )
        faults.clear()
        await db.execute(
            "UPDATE purchase_progress SET next_attempt_at=clock_timestamp()-interval '1 second' WHERE order_id=$1",
            UUID(p["id"]),
        )
        stop()
        start()
        for _ in range(100):
            r = await c.get("/shop/purchases/" + p["id"], headers=headers)
            if r.json()["state"] == "fulfilled":
                break
            await asyncio.sleep(0.1)
        assert r.json()["state"] == "fulfilled", r.text
        results["premium_recovered"] = r.json()
        # Both zero and partial internal balances produce truthful customer quantities.
        for initial in [0, 1]:
            await db.execute(
                "INSERT INTO hearts(user_id,hearts,last_refill) VALUES($1,$2,clock_timestamp()) ON CONFLICT(user_id) DO UPDATE SET hearts=$2,last_refill=clock_timestamp()",
                foo,
                initial,
            )
            o = await offer("hearts")
            s = await buy(o)
            assert s["state"] == "fulfilled", s
            statement = await c.get(f"/shop/purchases/{o['id']}/documents/fulfillment", headers=headers)
            body = await c.get(f"/shop/purchases/{o['id']}/documents/confirmation", headers=headers)
            actual = await c.get("/shop/hearts/me", headers=headers)
            results["hearts_" + str(initial)] = {
                "offer": o,
                "status": s,
                "statement": statement.text,
                "confirmation": body.text,
                "actual_heart_api": actual.json(),
            }
            assert s["fulfillment"]["added"] == 6 - initial and s["fulfillment"]["hearts_after"] == 6
            assert ("3 Herzen bereitgestellt" if initial == 0 else "2,5 Herzen bereitgestellt") in statement.text
            assert (
                "3 Herzen" in o["product"]["description"]
                and "zusätzlichen Versuchen" not in o["product"]["description"]
            )
            (EVID / f"actual-hearts-{initial}-confirmation.txt").write_bytes(body.content)
            (EVID / f"actual-hearts-{initial}-fulfillment.txt").write_bytes(statement.content)
            print(
                "PASS actual accepted/downloaded hearts use exact half-heart conversion, unchanged arithmetic:",
                initial,
                "->",
                s["fulfillment"]["hearts_after"],
                "added",
                s["fulfillment"]["added"],
                flush=True,
            )
        full = await c.post("/shop/purchases/offers/hearts", headers=headers)
        assert full.status_code == 412, full.text
        results["full_hearts_offer_status"] = full.status_code
        # Browser starts with one raw half-heart and an existing bought period.
        await db.execute("UPDATE hearts SET hearts=1,last_refill=clock_timestamp() WHERE user_id=$1", foo)
        await db.execute("UPDATE users SET email='l1-fix-bar@example.invalid',email_verified=true WHERE id=$1", bar)
        await db.execute(
            "INSERT INTO hearts(user_id,hearts,last_refill) VALUES($1,1,clock_timestamp()) ON CONFLICT(user_id) DO UPDATE SET hearts=1,last_refill=clock_timestamp()",
            bar,
        )
        await db.execute(
            "INSERT INTO coins(user_id,coins,withheld_coins) VALUES($1,100000,0) ON CONFLICT(user_id) DO UPDATE SET coins=100000",
            bar,
        )
        creds = {
            "base": "http://127.0.0.1:55872",
            "uid": str(foo),
            "token": token(foo),
            "other": token(bar),
            "recipient": address,
        }
        (EVID / "credentials.json").write_text(json.dumps(creds))
    (EVID / "independent-core-results.json").write_text(json.dumps(results, default=str, indent=2))
    await db.close()

    class Control(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            payload = creds if self.path == "/credentials" else {"mails": mails, "smtp_fault": faults.get(address)}
            raw = json.dumps(payload).encode()
            self.send_response(200)
            self.end_headers()
            self.wfile.write(raw)

        def do_POST(self):
            data = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
            if data.get("reject"):
                faults[address] = "reject"
            else:
                faults.clear()
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"{}")

    control = http.server.ThreadingHTTPServer(("127.0.0.1", 55876), Control)
    threading.Thread(target=control.serve_forever, daemon=True).start()
    print("READY independent core/browser fixture control55876", flush=True)
    if os.getenv("L1_BROWSER_HOLD"):
        await asyncio.to_thread(input)
    control.shutdown()
    control.server_close()


try:
    asyncio.run(main())
finally:
    stop()
    cache.terminate()
    cache.wait(timeout=10)
    smtp.shutdown()
    smtp.server_close()
    for i, raw in enumerate(captured_raw):
        (EVID / f"actual-smtp-{i}.eml").write_bytes(raw)
    (EVID / "smtp-messages.json").write_text(json.dumps(mails, default=str, indent=2))
