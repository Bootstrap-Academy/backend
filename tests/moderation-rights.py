"""Actual retained purchase originals, owned finance downloads and self-erasure.
Synthetic local render/export/delete boundaries are explicitly not service proofs.
"""

import asyncio, importlib.util, json, threading, subprocess
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from uuid import UUID, uuid4
import asyncpg, httpx

spec = importlib.util.spec_from_file_location("fixture", Path(__file__).with_name("moderation-http.py"))
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)


class Boundary(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def reply(self, value, typ="application/json"):
        self.send_response(200)
        self.send_header("Content-Type", typ)
        self.end_headers()
        self.wfile.write(value)

    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length", 0)))
        (
            self.reply(b"%PDF-1.4\nSynthetic local renderer\n%%EOF", "application/pdf")
            if self.path.endswith("html_to_pdf")
            else self.reply(b"[]")
        )

    def do_GET(self):
        self.reply(b"{}" if self.path.endswith("/export") else b"[]")

    def do_DELETE(self):
        self.reply(b"true")


r_spec = importlib.util.spec_from_file_location("renewal_fixture", Path(__file__).with_name("moderation-renewal.py"))
r_fixture = importlib.util.module_from_spec(r_spec)
r_spec.loader.exec_module(r_fixture)


async def main():
    servers = []
    db = None
    try:
        config = f.BASE / "rights-fixture.toml"
        config.write_text("[purchase.provision_window_seconds]\npremium_monthly=3600\n")
        f.ENV["ACADEMY_CONFIG"] = str(config) + ":" + f.ENV["ACADEMY_CONFIG"]
        for port in [55904, 55905, 55906, 55907]:
            server = ThreadingHTTPServer(("127.0.0.1", port), Boundary)
            server.daemon_threads = True
            threading.Thread(target=server.serve_forever, daemon=True).start()
            servers.append(server)
        smtp = f.SMTP(("127.0.0.1", 55903), r_fixture.AcceptedSMTP)
        threading.Thread(target=smtp.serve_forever, daemon=True).start()
        servers.append(smtp)
        f.start(
            [
                "/nix/store/d4lznfvcd8zqxn4hc9lpw0dvfri8p4c0-valkey-9.1.1/bin/valkey-server",
                "--port",
                "55902",
                "--bind",
                "127.0.0.1",
                "--save",
                "",
                "--appendonly",
                "no",
            ],
            "rights-valkey.log",
        )
        await f.wait_port(55902)
        f.start([str(f.ROOT / "target/debug/academy"), "serve"], "rights-backend.log")
        await f.wait_port(55901)
        db = await asyncpg.connect("postgresql://l2test@127.0.0.1:55900/l2backend")
        async with httpx.AsyncClient(base_url="http://127.0.0.1:55901", timeout=25) as c:

            async def cli(*args):
                p = await asyncio.create_subprocess_exec(
                    str(f.ROOT / "target/debug/academy"),
                    *args,
                    cwd=f.ROOT,
                    env=f.ENV,
                    stdout=asyncio.subprocess.PIPE,
                    stderr=asyncio.subprocess.STDOUT,
                )
                out = await p.communicate()
                assert p.returncode == 0, out

            name = "rights_" + uuid4().hex[:10]
            await cli("admin", "user", "create", "--verified", name, name + "@example.invalid", "synthetic password")
            login = await c.post("/auth/sessions", json={"name_or_email": name, "password": "synthetic password"})
            assert login.status_code == 200, login.text
            uid = UUID(login.json()["user"]["id"])
            user = {"Authorization": "Bearer " + login.json()["access_token"]}
            await cli("admin", "coin", "add", str(uid), "--", "50000")
            r = await c.post("/shop/purchases/offers/premium_monthly", headers=user)
            assert r.status_code == 200, r.text
            offer = r.json()["offer"]
            oid = UUID(offer["id"])
            r = await c.post(
                "/shop/purchases/accept",
                headers=user,
                json={
                    "order_id": str(oid),
                    "offer_hash": offer["hash"],
                    "accepted": True,
                    "early_performance_requested": True,
                },
            )
            assert r.status_code == 200, r.text
            originals = {}
            for kind in [
                "terms",
                "withdrawal",
                "confirmation",
                "fulfillment",
                "fulfillment-original",
                "timing",
                "timing-original",
            ]:
                response = await c.get(f"/shop/purchases/{oid}/documents/{kind}", headers=user)
                assert response.status_code == 200, (kind, response.status_code, response.text)
                originals[kind] = response.content
            assert originals["terms"] == (f.ROOT / "academy_assets/assets/email/agb-2026-09-r2.pdf").read_bytes()
            # Archive-only invoice control retains unknown capture, exactly as recorded.
            number = await db.fetchval("SELECT nextval('invoice_number')")
            order = "L2" + uuid4().hex[:16]
            invoice = b"%PDF-1.4\nSynthetic retained ORIGINAL invoice\n%%EOF"
            await db.execute(
                "INSERT INTO paypal_coin_orders(id,user_id,coins,created_at,invoice_number) VALUES($1,$2,1000,clock_timestamp(),$3)",
                order,
                uid,
                number,
            )
            await db.execute(
                "INSERT INTO invoice_originals(invoice_number,pdf,provenance) VALUES($1,$2,$3)",
                f"R{number:07}",
                invoice,
                "Synthetic original archive",
            )
            customer = await db.fetchval("SELECT nextval('user_number')")
            await db.execute(
                "INSERT INTO user_numbers(user_id,number) VALUES($1,$2) ON CONFLICT(user_id) DO NOTHING", uid, customer
            )
            customer = await db.fetchval("SELECT number FROM user_numbers WHERE user_id=$1", uid)
            credit = b"%PDF-1.4\nSynthetic owned archived credit note\n%%EOF"
            directory = f.BASE / "credit-notes"
            directory.mkdir(exist_ok=True)
            (directory / f"G202608-{customer}.pdf").write_bytes(credit)
            actor = uuid4()
            case = uuid4()
            await db.fetchval(
                "SELECT backend_moderation('open',$1,$2::jsonb)",
                actor,
                json.dumps(
                    {
                        "id": str(case),
                        "target_id": str(uid),
                        "source": "own_review",
                        "private_evidence": {"facts": "Synthetic retained rights"},
                    }
                ),
            )
            command = {
                "case_id": str(case),
                "request_key": str(uuid4()),
                "expected_revision": 0,
                "outcome": "restrict",
                "rationale": "Synthetic account restriction",
                "ground": "Synthetic security ground",
                "rule_version": "Synthetic only",
                "automation": "Synthetic human command",
                "scope": "Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten",
                "redress": "At least six calendar months human review",
                "misconduct_facts": "Synthetic actual evidence",
                "proportionality": "Synthetic limited measure",
                "hearing": "Synthetic urgent assessment",
            }
            decision = json.loads(
                await db.fetchval("SELECT backend_moderation('decide',$1,$2::jsonb)::text", actor, json.dumps(command))
            )
            proof = await c.post(
                "/auth/moderation/access/password", json={"name_or_email": name, "password": "synthetic password"}
            )
            assert proof.status_code == 200, proof.text
            rights = {"x-moderation-capability": proof.json()["capability"]}
            assert (await c.get("/auth/session", headers=user)).status_code == 401
            for kind, expected in originals.items():
                response = await c.get(f"/auth/moderation/purchases/{oid}/documents/{kind}", headers=rights)
                assert response.status_code == 200 and response.content == expected, (
                    kind,
                    response.status_code,
                    response.text,
                )
                assert ("application/pdf" if kind in ["terms", "withdrawal"] else "text/plain") in response.headers[
                    "content-type"
                ]
            r = await c.get(f"/auth/moderation/finance/invoice/{number}/0", headers=rights)
            assert r.status_code == 200 and r.content == invoice, r.text
            r = await c.get("/auth/moderation/finance/credit-note/2026/8", headers=rights)
            assert r.status_code == 200 and r.content == credit, r.text
            assert await db.fetchval("SELECT captured_at IS NULL FROM paypal_coin_orders WHERE id=$1", order)
            foreign = await c.post(
                "/auth/moderation/access/password", json={"name_or_email": "foo", "password": "foo password"}
            )
            assert foreign.status_code == 200
            foreignh = {"x-moderation-capability": foreign.json()["capability"]}
            for path in [f"/purchases/{oid}/documents/terms", f"/finance/invoice/{number}/0"]:
                r = await c.get("/auth/moderation" + path, headers=foreignh)
                assert r.status_code == 404, (r.status_code, r.text)
            print(
                "PASS actual disabled recipient gets byte-identical seven purchase documents, owned original invoice/credit PDF; foreign owner denied and unknown capture stays unknown",
                flush=True,
            )
            # Separate captured purchase observation solely for a real refund-claim receipt
            # during synthetic self-erasure; the unknown original above stays untouched.
            await db.execute(
                "INSERT INTO paypal_coin_orders(id,user_id,coins,created_at,captured_at,invoice_number) VALUES($1,$2,1000,clock_timestamp(),clock_timestamp(),nextval('invoice_number'))",
                "C" + uuid4().hex[:16],
                uid,
            )
            await db.execute("UPDATE coins SET coins=500 WHERE user_id=$1", uid)
            await db.execute(
                "UPDATE user_invoice_info SET business=false,first_name='Synthetic',last_name='Recipient',street='Fixture 1',zip_code='00000',city='Fixture',country='DE' WHERE user_id=$1",
                uid,
            )
            before = await db.fetchval(
                "SELECT public_statement::text FROM moderation_decisions WHERE id=$1", UUID(decision["decision_id"])
            )
            r = await c.delete("/auth/moderation/account", headers=rights)
            assert r.status_code == 200, r.text
            assert not await db.fetchval("SELECT EXISTS(SELECT 1 FROM users WHERE id=$1)", uid)
            record = await db.fetchrow("SELECT * FROM financial_documents WHERE number=$1", f"S{customer}")
            assert (
                record
                and record["user_id"] is None
                and record["settled_at"] is None
                and record["coins"] == 500
                and record["gross_total_cents"] == 500
            ), record
            assert (f.BASE / "final-statements" / f"S{customer}.pdf").exists()
            assert (
                await db.fetchval(
                    "SELECT public_statement::text FROM moderation_decisions WHERE id=$1", UUID(decision["decision_id"])
                )
                == before
            )
            assert not await db.fetchval(
                "SELECT EXISTS(SELECT 1 FROM moderation_holds WHERE case_id=$1 AND active AND NOT authority_order)",
                case,
            )
            assert (
                await db.fetchval("SELECT pdf FROM invoice_originals WHERE invoice_number=$1", f"R{number:07}")
                == invoice
            )
            r = await c.get(f"/auth/moderation/purchases/{oid}/documents/terms", headers=rights)
            assert r.status_code == 200 and r.content == originals["terms"]
            r = await c.post(
                "/auth/moderation/complaints/backend",
                headers=rights,
                json={
                    "id": str(uuid4()),
                    "decision_id": decision["decision_id"],
                    "text": "Synthetic appeal after legitimate erasure",
                },
            )
            assert r.status_code == 200, r.text
            print(
                "PASS actual restricted self-erasure preserves immutable case/purchase/invoice originals, ends ordinary hold, records unsettled500coin final statement/archive and permits retained appeal without account recreation",
                flush=True,
            )
    finally:
        if db:
            await db.close()
        for p in reversed(f.CHILDREN):
            if p.poll() is None:
                p.terminate()
            try:
                p.wait(timeout=15)
            except subprocess.TimeoutExpired:
                p.kill()
                p.wait()
        for s in servers:
            s.shutdown()
            s.server_close()


if __name__ == "__main__":
    asyncio.run(main())
