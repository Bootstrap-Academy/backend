"""Opt-in T12 local synthetic HTTP/SMTP/PostgreSQL probe. Requires T12_STATE; never production."""

import asyncio, json, os, socket, socketserver, subprocess, threading, time
from pathlib import Path
from uuid import UUID, uuid4
from datetime import datetime, timedelta, timezone
from email import policy
from email.parser import BytesParser
import asyncpg, httpx

ROOT = Path(__file__).resolve().parents[1]
state = json.loads(Path(os.environ["T12_STATE"]).read_text())
base = Path(state["base"])
assert base.name.startswith("bootstrap-t12-")
env = os.environ | {"ACADEMY_CONFIG": f"{state['config']}:{ROOT}/config.dev.toml", "RUST_LOG": "warn"}
server = None
faults = {}
mails = []


class Sink(socketserver.StreamRequestHandler):
    def handle(self):
        self.wfile.write(b"220 local synthetic SMTP\r\n")
        recipient = ""
        while line := self.rfile.readline():
            command = line.decode(errors="replace").strip()
            verb = command.split(" ")[0].upper()
            if verb in ["EHLO", "HELO"]:
                self.wfile.write(b"250-localhost\r\n250 8BITMIME\r\n")
            elif verb == "MAIL":
                self.wfile.write(b"250 OK\r\n")
            elif verb == "RCPT":
                recipient = command.split(":", 1)[1].strip("<>")
                self.wfile.write(b"250 OK\r\n")
            elif verb == "DATA":
                self.wfile.write(b"354 send data\r\n")
                lines = []
                while (line := self.rfile.readline()) not in [b".\r\n", b""]:
                    lines.append(line)
                message = BytesParser(policy=policy.default).parsebytes(b"".join(lines))
                fault = faults.get(recipient)
                if fault == "reject":
                    self.wfile.write(b"451 temporary synthetic rejection\r\n")
                    continue
                mails.append(
                    {
                        "recipient": recipient,
                        "id": str(message["Message-ID"]),
                        "subject": str(message["Subject"]),
                        "body": message.get_body(preferencelist=("plain",)).get_content(),
                    }
                )
                if fault == "lost_ack":
                    faults.pop(recipient, None)
                    return
                self.wfile.write(b"250 accepted\r\n")
            elif verb == "QUIT":
                self.wfile.write(b"221 bye\r\n")
                return
            else:
                self.wfile.write(b"250 OK\r\n")


class SMTP(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


smtp = SMTP(("127.0.0.1", 55874), Sink)
threading.Thread(target=smtp.serve_forever, daemon=True).start()
cache = subprocess.Popen(
    [
        "/nix/store/yaw5nj10i1kbg50mn2d5787yhis68sji-valkey-9.1.1/bin/valkey-server",
        "--port",
        "55873",
        "--bind",
        "127.0.0.1",
        "--save",
        "",
        "--appendonly",
        "no",
    ],
    stdout=(base / "valkey.log").open("w"),
    stderr=subprocess.STDOUT,
)


def start():
    global server
    server = subprocess.Popen(
        [str(ROOT / "target/debug/academy"), "serve"],
        cwd=ROOT,
        env=env,
        stdout=(base / "http.log").open("a"),
        stderr=subprocess.STDOUT,
    )
    for _ in range(100):
        try:
            with socket.create_connection(("127.0.0.1", 55872), timeout=0.1):
                return
        except OSError:
            time.sleep(0.05)
    raise RuntimeError("Backend startup failed")


def stop():
    global server
    if server:
        server.terminate()
        server.wait(timeout=15)
        server = None


def token(uid, admin=False):
    data = {
        "uid": str(uid),
        "sid": "11111111-1111-4111-8111-111111111111",
        "rt": "01" * 32,
        "data": {"admin": admin, "email_verified": True, "mfa": True},
    }
    return subprocess.check_output(
        [str(ROOT / "target/debug/academy"), "jwt", "sign", json.dumps(data)], env=env, text=True
    ).strip()


async def main():
    for args in [["migrate", "reset", "--force"], ["migrate", "up"], ["migrate", "demo", "--force"]]:
        subprocess.run(
            [str(ROOT / "target/debug/academy"), *args], env=env, cwd=ROOT, check=True, stdout=subprocess.DEVNULL
        )
    db = await asyncpg.connect("postgresql://morpheus@127.0.0.1:55572/t12backend")
    foo = UUID("a8d95e0f-71ae-4c49-995e-695b7c93848c")
    bar = UUID("94d0e3ca-bf16-486b-a172-b87f4bcbd039")
    admin = UUID("e3f8a50a-a5a3-444a-9026-77336f716d03")

    async def seed(uid):
        pid, aid = uuid4(), uuid4()
        until = datetime.now(timezone.utc) + timedelta(days=10)
        email = await db.fetchval("SELECT email FROM users WHERE id=$1", uid)
        await db.execute(
            "INSERT INTO premium(id,user_id,since,until) VALUES($1,$2,$3,$4)",
            pid,
            uid,
            until - timedelta(days=30),
            until,
        )
        await db.execute(
            "INSERT INTO premium_renewal_agreements(id,user_id,received_at,offer_id,monthly_price,recipient,document,terms_pdf,withdrawal_pdf,paid_period_id,confirmation_deadline) VALUES($1,$2,clock_timestamp(),'synthetic',777,$3,'synthetic agreement',$4,$4,$5,$6)",
            aid,
            uid,
            email,
            b"test",
            pid,
            until,
        )
        await db.execute("INSERT INTO premium_renewal_delivery(agreement_id,sent_at) VALUES($1,clock_timestamp())", aid)
        await db.execute(
            "INSERT INTO premium_subscriptions(user_id,plan,agreement_id) VALUES($1,'monthly',$2)", uid, aid
        )
        return pid, aid, until, email

    await db.execute("UPDATE users SET email='t12-bar@example.invalid',email_verified=true WHERE id=$1", bar)
    fp, fa, fu, fe = await seed(foo)
    bp, ba, bu, be = await seed(bar)
    start()
    headers = {"Authorization": "Bearer " + token(admin, True)}
    fheaders = {"Authorization": "Bearer " + token(foo)}
    keys = {
        "id",
        "kind",
        "received_at",
        "name",
        "email",
        "contract",
        "contract_designation",
        "cancellation_type",
        "details",
        "requested_end",
    }

    def body(email, **extra):
        return {
            "name": "T12 synthetic declaration",
            "email": email,
            "contract": "PREMIUM",
            "cancellation_type": "ORDINARY",
            "details": "Original reason / order № 12",
            "request_key": {"id": str(uuid4()), "secret": str(uuid4())},
            **extra,
        }

    async with httpx.AsyncClient(base_url="http://127.0.0.1:55872", timeout=40) as client:
        p = body(fe, requested_end=(fu + timedelta(days=70)).isoformat())
        r = await client.post("/contracts/cancellations", json=p)
        assert r.status_code == 200, r.text
        assert set(r.json()["declaration"]) == keys
        assert await db.fetchval("SELECT count(*) FROM premium_subscriptions") == 2
        known = r.json()
        lookup = await client.post("/contracts/receipts", json=p["request_key"])
        assert lookup.json() == known
        assert (
            await client.post("/contracts/receipts", json={**p["request_key"], "secret": str(uuid4())})
        ).status_code == 404
        rs = await asyncio.gather(*[client.post("/contracts/cancellations", json=p) for _ in range(6)])
        assert all(x.status_code == 200 and x.json()["declaration"] == known["declaration"] for x in rs)
        assert (
            await db.fetchval("SELECT count(*) FROM contract_declarations WHERE id=$1", UUID(p["request_key"]["id"]))
            == 1
        )
        assert (
            await client.post("/contracts/cancellations", json={**p, "details": "Changed declaration"})
        ).status_code == 409
        assert (
            await client.post("/contracts/cancellations", json={**p, "renewal_agreement_id": str(uuid4())})
        ).status_code == 409
        for email in ["unregistered-t12@example.invalid", fe]:
            r = await client.post("/contracts/cancellations", json=body(email))
            assert r.status_code == 200, r.text
            assert set(r.json()["declaration"]) == keys
        assert await db.fetchval("SELECT count(*) FROM premium_subscriptions") == 2
        print(
            "PASS generic known/unknown/paid receipts; email alone has no effects; capability lookup; six concurrent duplicates; changed payload conflict",
            flush=True,
        )
        scheduled = body(be, renewal_agreement_id=str(ba), requested_end=(bu + timedelta(days=15)).isoformat())
        r = await client.post("/contracts/cancellations", json=scheduled)
        assert r.status_code == 200, r.text
        assert await db.fetchval("SELECT agreement_id FROM premium_subscriptions WHERE user_id=$1", bar) == ba
        listing = (await client.get("/contracts/declarations", headers=headers)).json()
        item = next(x for x in listing["declarations"] if x["id"] == scheduled["request_key"]["id"])
        assert item["details"] == scheduled["details"]
        evidence = json.loads(item["operational_evidence"])
        assert evidence["schedule"]["agreement_id"] == str(ba)
        assert next(m for m in evidence["messages"] if m["kind"] == "receipt")["requested_agreement_id"] == str(ba)
        assert (
            await client.post("/contracts/cancellations", json={**scheduled, "renewal_agreement_id": str(uuid4())})
        ).status_code == 409
        assert evidence["paid_period_observations"]
        assert item["delivery"]
        # API administrative processing uses the original date and requires explicit verification.
        path = "/contracts/declarations/" + p["request_key"]["id"]
        assert (await client.patch(path, headers=headers, json={})).status_code == 400
        action = {
            "identity_verified": True,
            "action": "SCHEDULE_PREMIUM_CANCELLATION",
            "verified_user_id": str(foo),
            "renewal_agreement_id": str(fa),
            "note": "Synthetic identity and original agreement checked; requested date recorded.",
        }
        r = await client.patch(path, headers=headers, json=action)
        assert r.status_code == 200, r.text
        assert r.json()["effective_end"] is None
        assert (await client.patch(path, headers=headers, json=action)).status_code == 409
        assert (await client.get("/contracts/declarations")).status_code == 401
        assert (await client.get("/contracts/declarations", headers=fheaders)).status_code == 403
        print(
            "PASS exact-agreement future scheduling, admin complete evidence, required verification, duplicate action fencing and auth isolation",
            flush=True,
        )
        # Lost acknowledgement: local SMTP accepts bytes, closes without 250; receipt remains queued.
        lost = body("lost-t12@example.invalid")
        faults[lost["email"]] = "lost_ack"
        r = await client.post("/contracts/cancellations", json=lost)
        assert r.status_code == 200, r.text
        assert r.json()["confirmation_email_sent"] is False
        lost_id = UUID(lost["request_key"]["id"])
        before = await db.fetchrow(
            "SELECT attempts,accepted_at,next_attempt_at FROM contract_delivery WHERE declaration_id=$1 AND kind='receipt'",
            lost_id,
        )
        assert before["attempts"] == 1 and before["accepted_at"] is None
        reject = body("reject-t12@example.invalid")
        faults[reject["email"]] = "reject"
        r = await client.post("/contracts/cancellations", json=reject)
        assert r.status_code == 200 and not r.json()["confirmation_email_sent"]
        reject_id = UUID(reject["request_key"]["id"])
        stop()
        faults.clear()
        # Wall-clock lease expiry; workers restart independently of browser submission.
        seconds = max(0, (before["next_attempt_at"] - datetime.now(timezone.utc)).total_seconds()) + 1
        print(f"WAIT real delivery lease expiry ({seconds:.1f}s), then restart", flush=True)
        await asyncio.sleep(seconds)
        start()
        for _ in range(100):
            accepted = await db.fetchval(
                "SELECT count(*) FROM contract_delivery WHERE declaration_id=ANY($1::uuid[]) AND kind='receipt' AND accepted_at IS NOT NULL",
                [lost_id, reject_id],
            )
            if accepted == 2:
                break
            await asyncio.sleep(0.2)
        assert accepted == 2
        sent = [m for m in mails if m["recipient"] == lost["email"]]
        assert len(sent) == 2, sent
        assert sent[0]["id"] == sent[1]["id"]
        assert sent[0]["body"] == sent[1]["body"]
        assert await db.fetchval("SELECT count(*) FROM contract_declarations WHERE id=$1", lost_id) == 1
        response = await client.post("/contracts/receipts", json=lost["request_key"])
        assert response.status_code == 200 and response.json()["confirmation_email_sent"]
        assert set(response.json()["declaration"]) == keys
        print(
            "PASS SMTP rejection and accepted/lost-250, real lease expiry, process restart, immutable content and stable Message-ID, no duplicate declaration",
            flush=True,
        )
        # Processing fields and account facts never enter the protected public receipt either.
        await db.execute(
            "UPDATE contract_declarations SET effective_end=clock_timestamp(),processing_note='PRIVATE STAFF FACT' WHERE id=$1",
            lost_id,
        )
        response = await client.post("/contracts/receipts", json=lost["request_key"])
        assert set(response.json()["declaration"]) == keys
        assert "PRIVATE" not in response.text
        print("PASS retained lookup redacts later processing data", flush=True)
    # Fairness under failed acknowledgement COMMITs: the 101st healthy item must progress.
    stop()
    await db.execute("UPDATE contract_delivery SET next_attempt_at=clock_timestamp() WHERE accepted_at IS NULL")
    subprocess.run(
        [str(ROOT / "target/debug/academy"), "task", "retry-contract-confirmations"],
        env=env,
        cwd=ROOT,
        check=True,
        stdout=subprocess.DEVNULL,
    )
    ids = [uuid4() for _ in range(101)]
    for i, ident in enumerate(ids):
        address = f"fair-{i}@example.invalid" if i < 100 else "healthy-101@example.invalid"
        await db.execute(
            "INSERT INTO contract_declarations(id,kind,received_at,name,email,contract,details) VALUES($1,'withdrawal',clock_timestamp(),'Synthetic fairness',$2,'other','Fairness test')",
            ident,
            address,
        )
        await db.execute(
            "INSERT INTO contract_delivery(declaration_id,kind,recipient,subject,body,next_attempt_at) VALUES($1,'receipt',$2,'Fairness','Stable fairness body',clock_timestamp()+($3::int*interval '1 microsecond')-interval '1 hour')",
            ident,
            address,
            i,
        )
    await db.execute(
        """CREATE FUNCTION t12_ack_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.accepted_at IS NOT NULL AND NEW.recipient LIKE 'fair-%' THEN RAISE EXCEPTION 'synthetic acknowledgement COMMIT failure'; END IF; RETURN NEW; END $$;
 CREATE CONSTRAINT TRIGGER t12_ack_fault AFTER UPDATE ON contract_delivery DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION t12_ack_fault();"""
    )
    counts = []
    for number in range(3):
        before_count = len(mails)
        result = await asyncio.to_thread(
            subprocess.run,
            [str(ROOT / "target/debug/academy"), "task", "retry-contract-confirmations"],
            env=env,
            cwd=ROOT,
            stdout=(base / f"fairness-{number}.log").open("w"),
            stderr=subprocess.STDOUT,
        )
        counts.append(len(mails) - before_count)
        if number == 0:
            assert result.returncode != 0
    assert counts == [100, 1, 0], counts
    assert await db.fetchval("SELECT accepted_at IS NOT NULL FROM contract_delivery WHERE declaration_id=$1", ids[-1])
    assert (
        await db.fetchval(
            "SELECT count(*) FROM contract_delivery WHERE declaration_id=ANY($1::uuid[]) AND attempts=1 AND accepted_at IS NULL",
            ids[:-1],
        )
        == 100
    )
    await db.execute("DROP TRIGGER t12_ack_fault ON contract_delivery; DROP FUNCTION t12_ack_fault()")
    # Test cleanup may expire synthetic leases; the HTTP recovery probe above used actual wall time.
    await db.execute("UPDATE contract_delivery SET next_attempt_at=clock_timestamp() WHERE accepted_at IS NULL")
    await asyncio.to_thread(
        subprocess.run,
        [str(ROOT / "target/debug/academy"), "task", "retry-contract-confirmations"],
        env=env,
        cwd=ROOT,
        check=True,
        stdout=subprocess.DEVNULL,
    )
    print(
        "PASS three fresh worker processes send 100/1/0 under 100 failed acknowledgement COMMITs; healthy item 101 progresses; attempts durable",
        flush=True,
    )
    await db.close()
    (base / "smtp.json").write_text(json.dumps(mails, ensure_ascii=False, indent=2))


try:
    asyncio.run(main())
finally:
    stop()
    cache.terminate()
    cache.wait(timeout=10)
    smtp.shutdown()
    smtp.server_close()
