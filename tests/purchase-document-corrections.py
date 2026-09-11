"""Additive document migration and actual owner/download/export regression.

Requires L1_DOCUMENT_TEST_STATE pointing to an OWNED synthetic fixture JSON
with pg/config/base, and L1_DOCUMENT_SOURCE_URL for that fixture's source DB.
Creates and removes a separate database/config/server. Never uses customer data.
"""

import asyncio
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import tempfile
from uuid import UUID, uuid4

import asyncpg
import httpx

ROOT = Path(__file__).resolve().parents[1]
STATE = json.loads(Path(os.environ["L1_DOCUMENT_TEST_STATE"]).read_text())
assert Path(STATE["base"]).name.startswith("l1-fix-2-services-")
SOURCE = os.environ["L1_DOCUMENT_SOURCE_URL"]
assert SOURCE.startswith("postgresql://morpheus@127.0.0.1:55730/")
E = Path(os.environ["L1_DOCUMENT_EVIDENCE"]).resolve()
E.mkdir(parents=True, exist_ok=True)
WORK = Path(tempfile.mkdtemp(prefix="l1-fix2-documents-"))
NAME = "l1doc_" + uuid4().hex
URL = SOURCE.rsplit("/", 1)[0] + "/" + NAME
PG = Path(STATE["pg"])
BIN = ROOT / "target/debug/academy"
config = WORK / "config.toml"
s = (
    Path(STATE["config"])
    .read_text()
    .replace(SOURCE, URL)
    .replace("127.0.0.1:55732", "127.0.0.1:55738")
    .replace("redis://127.0.0.1:55731/0", "redis://127.0.0.1:55731/11")
)
config.write_text(s)
ENV = os.environ | {
    "ACADEMY_CONFIG": f"{config}:{ROOT}/config.dev.toml",
    "RUST_LOG": "warn",
}
server = None


def clear_test_cache():
    # DB 11 is reserved solely for this owned document fixture.
    with socket.create_connection(("127.0.0.1", 55731)) as connection:
        connection.sendall(b"*2\r\n$6\r\nSELECT\r\n$2\r\n11\r\n*1\r\n$7\r\nFLUSHDB\r\n")
        reply = connection.makefile("rb")
        assert reply.readline() == b"+OK\r\n"
        assert reply.readline() == b"+OK\r\n"


def cli(*args, failure=False):
    r = subprocess.run(
        [str(BIN), *args], cwd=ROOT, env=ENV, capture_output=True, text=True
    )
    with (E / "migration-cli.log").open("a") as f:
        f.write(
            (
                "synthetic JWT issued; token omitted\n"
                if args[:2] == ("jwt", "sign")
                else str(args) + "\n" + r.stdout + r.stderr
            )
        )
    assert (r.returncode != 0) if failure else (r.returncode == 0), r.stderr
    return r


def token(uid):
    data = {
        "uid": str(uid),
        "sid": str(uuid4()),
        "rt": "01" * 32,
        "data": {"admin": False, "email_verified": True, "mfa": True},
    }
    return cli("jwt", "sign", json.dumps(data)).stdout.strip()


async def snapshot(db):
    tables = await db.fetch(
        "SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename NOT IN ('_migrations','purchase_document_corrections') ORDER BY tablename"
    )
    out = {}
    for row in tables:
        name = row["tablename"]
        rows = await db.fetch(f'SELECT to_jsonb(t)::text AS data FROM "{name}" t')
        values = [json.loads(r["data"]) for r in rows]
        if name in ("purchase_fulfillments", "purchase_provision_observations"):
            for v in values:
                v.pop("statement_version", None)
        out[name] = sorted(values, key=lambda v: json.dumps(v, sort_keys=True))
    return out


async def main():
    global server
    clear_test_cache()
    source = await asyncpg.connect(SOURCE)
    template = await source.fetchrow(
        "SELECT o.*, f.result::text AS result FROM purchase_offers o JOIN purchase_fulfillments f ON f.order_id=o.id WHERE o.source='skills' LIMIT 1"
    )
    assert template, "Run a synthetic course purchase first"
    await source.close()
    subprocess.run(
        [
            str(PG / "createdb"),
            "-h",
            "127.0.0.1",
            "-p",
            "55730",
            "--encoding=UTF8",
            "--template=template0",
            NAME,
        ],
        check=True,
    )
    cli("migrate", "demo", "--force")
    cli("migrate", "down", "--count", "1")
    db = await asyncpg.connect(URL)
    uid = template["user_id"]
    when = "2026-09-08T01:00:00Z"
    definitions = [
        (
            "course",
            "skills",
            {"kind": "course_access_provided", "provided_at": when},
            f"Kurszugang bereitgestellt am {when}. Dies bestätigt die Freischaltung, nicht das Ansehen oder den Abschluss des Kurses.",
        ),
        (
            "coins",
            "paypal",
            {
                "kind": "coin_balance_provided",
                "provided_at": when,
                "balance": {"coins": 1337},
            },
            f"MorphCoins-Guthaben bereitgestellt am {when}. Bestand unmittelbar danach: 1337 MorphCoins.",
        ),
        (
            "premium_monthly",
            "backend",
            {"purchased_since": when, "purchased_until": "2026-10-08T01:00:00Z"},
            "Premium-Zeitraum: unveränderter zugeordneter Zeitraum.",
        ),
        (
            "hearts",
            "backend",
            {"added": 5, "hearts_after": 6},
            "2,5 Herzen bereitgestellt; Bestand unmittelbar danach: 3 Herzen.",
        ),
        (
            "webinar",
            "events",
            {"kind": "booking_access_provided", "provided_at": when},
            f"Buchungszugang spätestens am {when} als verfügbar beobachtet.",
        ),
        (
            "webinar",
            "events",
            {
                "kind": "booking_access_provided",
                "provided_at": when,
                "availability_observed_at": when,
                "timing_basis": "committed_candidate_v1",
            },
            f"Buchungszugang spätestens am {when} als verfügbar beobachtet.",
        ),
        (
            "unknown",
            "skills",
            {"kind": "unknown_source_record"},
            "Unbekannter historischer Wortlaut — keine neuen Tatsachen.",
        ),
    ]
    fixtures = []

    async def seed(definition, owner=uid):
        kind, source_name, result, detail = definition
        oid = uuid4()
        offer = json.loads(template["offer"])
        offer.update(id=str(oid), user_id=str(owner), source=source_name)
        offer["product"].update(kind=kind, reference=str(oid))
        offer["product"]["facts"] = (
            {"availability_protocol": "committed_candidate_v1"}
            if result.get("timing_basis")
            else {}
        )
        result = result | {"order_id": str(oid)}
        original = f"Bereitstellungsnachweis – Bootstrap Academy\nBestellung: {oid}\n{detail}\n"
        timing = f"Zeitnachweis zur Bereitstellung – Bootstrap Academy\nBestellung: {oid}\nBereitstellung vor der vereinbarten Frist belegt: true.\n"
        await db.execute(
            "INSERT INTO purchase_offers VALUES($1,$2,$3,$4::jsonb,$5,$6,$7,$8)",
            oid,
            owner,
            source_name,
            json.dumps(offer),
            template["terms_pdf"],
            template["withdrawal_pdf"],
            template["created_at"],
            template["expires_at"],
        )
        await db.execute(
            "INSERT INTO purchase_progress(order_id,state,review_reason,smtp_accepted_at) VALUES($1,'review','existing synthetic claim remains open',now())",
            oid,
        )
        await db.execute(
            "INSERT INTO purchase_acceptances(order_id,confirmation_body,message_metadata) VALUES($1,'Original accepted confirmation, not rewritten','{}')",
            oid,
        )
        await db.execute(
            "INSERT INTO purchase_fulfillments(order_id,result,statement) VALUES($1,$2::jsonb,$3)",
            oid,
            json.dumps(result),
            original,
        )
        await db.execute(
            "INSERT INTO purchase_provision_observations(order_id,observed_at,evidence,statement) VALUES($1,now(),$2::jsonb,$3)",
            oid,
            json.dumps(
                {
                    "committed_before_deadline_proven": True,
                    "fulfillment": result,
                    "observed_at": when,
                }
            ),
            timing,
        )
        fixtures.append(
            {
                "id": str(oid),
                "kind": kind,
                "owner": str(owner),
                "fulfillment": original,
                "timing": timing,
            }
        )
        return fixtures[-1]

    for definition in definitions:
        await seed(definition)
    await seed(
        definitions[0], uuid4()
    )  # Retained evidence of a deleted/missing account.
    before = await snapshot(db)
    cli("migrate", "up")
    assert await snapshot(db) == before, "Migration altered preexisting facts"
    corrections = [
        dict(r)
        for r in await db.fetch(
            "SELECT * FROM purchase_document_corrections ORDER BY order_id,document_kind"
        )
    ]
    assert len(corrections) == 2 * len(fixtures)
    for r in corrections:
        f = next(f for f in fixtures if f["id"] == str(r["order_id"]))
        assert (
            r["original_sha256"]
            == hashlib.sha256(f[r["document_kind"]].encode()).hexdigest()
        )
        assert (
            r["statement_sha256"] == hashlib.sha256(r["statement"].encode()).hexdigest()
        )
        assert "Korrektur und Einordnung" in r["statement"]
    cli("migrate", "up")
    assert [
        dict(r)
        for r in await db.fetch(
            "SELECT * FROM purchase_document_corrections ORDER BY order_id,document_kind"
        )
    ] == corrections
    cli("migrate", "down", "--count", "1", failure=True)
    assert await snapshot(db) == before
    # Late legacy-writer inserts receive an additive correction in the same
    # transaction; concurrent callers cannot replace its first retained bytes.
    late = await seed(definitions[1])
    oid = UUID(late["id"])

    async def catchup():
        c = await asyncpg.connect(URL)
        await c.execute(
            "SELECT correct_purchase_document(order_id,'fulfillment',statement,result) FROM purchase_fulfillments WHERE order_id=$1",
            oid,
        )
        await c.close()

    await asyncio.gather(*(catchup() for _ in range(6)))
    assert (
        await db.fetchval(
            "SELECT count(*) FROM purchase_document_corrections WHERE order_id=$1", oid
        )
        == 2
    )
    try:
        await db.execute(
            "UPDATE purchase_document_corrections SET statement='rewrite' WHERE order_id=$1",
            oid,
        )
        raise AssertionError("Correction was mutable")
    except asyncpg.RaiseError:
        pass
    saved_before_http = await snapshot(db)
    server = subprocess.Popen(
        [str(BIN), "serve"],
        cwd=ROOT,
        env=ENV,
        stdout=(E / "http.log").open("w"),
        stderr=subprocess.STDOUT,
    )
    for _ in range(100):
        try:
            with socket.create_connection(("127.0.0.1", 55738), timeout=0.1):
                break
        except OSError:
            await asyncio.sleep(0.05)
    owner = {"Authorization": "Bearer " + token(uid)}
    other = {
        "Authorization": "Bearer " + token(UUID("94d0e3ca-bf16-486b-a172-b87f4bcbd039"))
    }
    results = []
    async with httpx.AsyncClient(
        base_url="http://127.0.0.1:55738", timeout=30
    ) as client:
        for f in fixtures:
            for kind in ["fulfillment", "timing"]:
                path = f"/shop/purchases/{f['id']}/documents/{kind}"
                if f["owner"] != str(uid):
                    assert (await client.get(path, headers=owner)).status_code == 404
                    continue
                copies = await asyncio.gather(
                    *(client.get(path, headers=owner) for _ in range(4))
                )
                assert all(
                    r.status_code == 200 and r.content == copies[0].content
                    for r in copies
                )
                current = copies[0]
                original = await client.get(path + "-original", headers=owner)
                assert (
                    original.status_code == 200 and original.content == f[kind].encode()
                )
                assert (
                    current.content != original.content
                    and "Korrektur und Einordnung" in current.text
                )
                assert hashlib.sha256(original.content).hexdigest() in current.text
                assert current.headers["content-type"].startswith("text/plain")
                assert original.headers["content-type"].startswith("text/plain")
                assert "no-store" in original.headers["cache-control"]
                for suffix in ["", "-original"]:
                    assert (
                        await client.get(path + suffix, headers=other)
                    ).status_code == 404
                    anonymous = await client.get(path + suffix)
                    assert anonymous.status_code == 404, (
                        anonymous.status_code,
                        anonymous.text,
                    )
                    assert original.text not in anonymous.text
                (E / f"{f['id']}-{kind}.txt").write_bytes(current.content)
                (E / f"{f['id']}-{kind}-original.txt").write_bytes(original.content)
                results.append(
                    {
                        "order": f["id"],
                        "kind": kind,
                        "original_sha256": hashlib.sha256(original.content).hexdigest(),
                        "current_sha256": hashlib.sha256(current.content).hexdigest(),
                    }
                )
        listing = await client.get("/shop/purchases", headers=owner)
        assert listing.status_code == 200
        assert all(
            set(r["document_corrections"]) == {"fulfillment", "timing"}
            for r in listing.json()
        )
        exported = await client.get(
            "/auth/users/" + str(uid) + "/export", headers=owner
        )
        assert exported.status_code == 200, exported.text
        payload = exported.json()
        (E / "account-export.json").write_text(json.dumps(payload, indent=2))

        def find(value):
            if isinstance(value, dict):
                if "purchase_evidence" in value:
                    return value["purchase_evidence"]
                for v in value.values():
                    r = find(v)
                    if r is not None:
                        return r
            if isinstance(value, list):
                for v in value:
                    r = find(v)
                    if r is not None:
                        return r

        purchases = find(payload)
        assert purchases is not None, payload.keys()
        for p in purchases:
            assert len(p["document_corrections"]) == 2
            original = p["fulfillment"]["statement"]
            correction = next(
                c
                for c in p["document_corrections"]
                if c["document_kind"] == "fulfillment"
            )
            assert (
                correction["original_sha256"]
                == hashlib.sha256(original.encode()).hexdigest()
            )
            timing_original = p["provision_timing_document"]["statement"]
            timing_correction = next(
                c for c in p["document_corrections"] if c["document_kind"] == "timing"
            )
            assert (
                timing_correction["original_sha256"]
                == hashlib.sha256(timing_original.encode()).hexdigest()
            )
    # Export writes its normal audit/rate-limit bookkeeping; verify the exact
    # purchase/financial/claim facts separately from that expected observation.
    after = await snapshot(db)
    for name, rows in saved_before_http.items():
        if name.startswith(("purchase_", "paypal_", "contract_")) or name in (
            "transactions",
            "coins",
            "premium",
            "hearts",
            "invoice_originals",
        ):
            assert after[name] == rows, name
    (E / "results.json").write_text(
        json.dumps(
            {
                "fixtures": fixtures,
                "downloads": results,
                "legacy_corrections": len(corrections),
                "late_catchup": True,
                "used_downgrade_refused": True,
                "original_facts_unchanged": True,
            },
            indent=2,
        )
    )
    # Recovery must not promote an arbitrary legacy marker to a source witness.
    # These synthetic retained rows isolate observation provenance; they are
    # not claims that an external meeting or earlier real lesson took place.
    server.terminate()
    server.wait(timeout=15)
    server = None
    protocol_cases = []
    for source_name, accepted_protocol in [
        ("skills", False),
        ("events", False),
        ("events", True),
    ]:
        oid = uuid4()
        offer = json.loads(template["offer"])
        offer.update(id=str(oid), user_id=str(uid), source=source_name)
        offer["product"]["facts"] = (
            {"availability_protocol": "committed_candidate_v1"}
            if accepted_protocol
            else {}
        )
        proof = {
            "kind": "booking_access_provided",
            "provided_at": when,
            "availability_observed_at": when,
            "timing_basis": "committed_candidate_v1",
        }
        await db.execute(
            "INSERT INTO purchase_offers VALUES($1,$2,$3,$4::jsonb,$5,$6,$7,$8)",
            oid,
            uid,
            source_name,
            json.dumps(offer),
            template["terms_pdf"],
            template["withdrawal_pdf"],
            template["created_at"],
            template["expires_at"],
        )
        await db.execute(
            "INSERT INTO purchase_progress(order_id,state,smtp_accepted_at) VALUES($1,'fulfilled',now())",
            oid,
        )
        await db.execute(
            "INSERT INTO purchase_acceptances(order_id,accepted_at,confirmation_body,message_metadata) VALUES($1,now()-interval '1 hour','Synthetic original confirmation','{}')",
            oid,
        )
        await db.execute(
            "INSERT INTO purchase_fulfillments(order_id,result,statement,statement_version) VALUES($1,$2::jsonb,'Synthetic retained source-report fixture',2)",
            oid,
            json.dumps(proof),
        )
        protocol_cases.append((oid, accepted_protocol))
    server = subprocess.Popen(
        [str(BIN), "serve"],
        cwd=ROOT,
        env=ENV,
        stdout=(E / "protocol-http.log").open("w"),
        stderr=subprocess.STDOUT,
    )
    for _ in range(200):
        if (
            await db.fetchval(
                "SELECT count(*) FROM purchase_provision_observations WHERE order_id=ANY($1::uuid[])",
                [x[0] for x in protocol_cases],
            )
            == 3
        ):
            break
        await asyncio.sleep(0.05)
    for _ in range(100):
        try:
            with socket.create_connection(("127.0.0.1", 55738), timeout=0.1):
                break
        except OSError:
            await asyncio.sleep(0.05)
    protocol_results = []
    async with httpx.AsyncClient(
        base_url="http://127.0.0.1:55738", timeout=30
    ) as client:
        for oid, valid in protocol_cases:
            row = await db.fetchrow(
                "SELECT evidence::text,statement FROM purchase_provision_observations WHERE order_id=$1",
                oid,
            )
            assert row
            proof = json.loads(row["evidence"])
            assert (
                proof["source_availability_observed_at"] is not None
            ) == valid, proof
            assert proof["committed_before_deadline_proven"] == valid, proof
            assert ("Zugangsdaten wurden bereits" in row["statement"]) == valid, row[
                "statement"
            ]
            downloaded = await client.get(
                f"/shop/purchases/{oid}/documents/timing", headers=owner
            )
            assert downloaded.status_code == 200 and downloaded.text == row["statement"]
            protocol_results.append(
                {"order": str(oid), "accepted_events_protocol": valid, "timing": proof}
            )
    (E / "source-protocol-results.json").write_text(
        json.dumps(protocol_results, indent=2)
    )
    print(
        "PASS source-marker-only Skills/legacy Events reports do not certify earlier availability; accepted Events protocol preserves its original observation bound",
        flush=True,
    )
    if os.environ.get("L1_DOCUMENT_BROWSER_WAIT"):
        (E / "browser-credentials.json").write_text(
            json.dumps(
                {
                    "base": "http://127.0.0.1:55738",
                    "uid": str(uid),
                    "token": owner["Authorization"].removeprefix("Bearer "),
                    "other": other["Authorization"].removeprefix("Bearer "),
                }
            )
        )
        print(
            "READY document browser fixture " + str(E / "browser-credentials.json"),
            flush=True,
        )
        await asyncio.to_thread(input)
        (E / "browser-credentials.json").unlink()
    await db.close()
    print(
        "PASS used migration preserves originals/facts; legacy and late reports receive immutable corrections; concurrent actual current/original owner downloads, MIME, privacy, export linkage and used downgrade guard",
        flush=True,
    )


try:
    asyncio.run(main())
finally:
    if server and server.poll() is None:
        server.terminate()
        server.wait(timeout=15)
    subprocess.run(
        [
            str(PG / "dropdb"),
            "-h",
            "127.0.0.1",
            "-p",
            "55730",
            "--if-exists",
            "--force",
            NAME,
        ],
        check=True,
    )
    clear_test_cache()
    shutil.rmtree(WORK)
    print("Cleanup: removed owned document database/config/server", flush=True)
