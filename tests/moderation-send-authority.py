"""Opt-in synthetic PostgreSQL controls for immediate send authority and disposal.

Run only against the explicitly owned disposable L2 database. SQL admission is
not an SMTP delivery claim; the separate actual-worker fixture checks SMTP.
"""

import asyncio, json, os
from uuid import UUID, uuid4
import asyncpg

DSN = os.environ.get("L2_DATABASE_URL", "postgresql://l2test@127.0.0.1:55900/l2backend")
assert "@127.0.0.1:" in DSN and DSN.endswith("/l2backend")


async def main():
    db = await asyncpg.connect(DSN)
    other = await asyncpg.connect(DSN)

    async def op(conn, name, body=None, actor=None):
        return json.loads(
            await conn.fetchval("SELECT backend_moderation($1,$2,$3::jsonb)::text", name, actor, json.dumps(body or {}))
        )

    def message(case=None, source="challenges", recipient=None, audience="notifier"):
        return {
            "source": source,
            "id": str(uuid4()),
            "case_id": str(case or uuid4()),
            "recipient": str(recipient or uuid4()),
            "audience": audience,
            "body": {"rationale": "Synthetic private case", "automation": "Human fixture", "redress": "Human review"},
            "contact": "old@example.invalid",
            "available_at": "2026-09-08T00:00:00Z",
        }

    def ack(admitted, status="transport_accepted"):
        return {k: admitted[k] for k in ["source", "id", "generation", "attempt_id"]} | {"status": status}

    async def claimed(msg):
        rows = await op(db, "claim_email")
        assert all(set(r) == {"source", "id", "generation"} for r in rows), rows
        return next(r for r in rows if r["id"] == msg["id"])

    try:
        await db.execute("UPDATE moderation_delivery SET next_attempt_at=clock_timestamp()+interval '1 day'")
        m = message()
        await op(db, "accept_delivery", m)
        q = await claimed(m)
        corrected = await op(
            db,
            "delivery_contact",
            {
                "source": m["source"],
                "id": m["id"],
                "contact": "current@example.invalid",
                "verification_evidence": "Synthetic individually verified recipient",
            },
            uuid4(),
        )
        assert corrected["corrected"] and corrected["in_progress_attempts"] == 0
        assert await op(db, "admit_email", q) is None
        q = await claimed(m)
        admitted = await op(db, "admit_email", q)
        assert admitted["contact"] == "current@example.invalid"
        assert await op(db, "admit_email", q) is None, "duplicate admission"
        corrected = await op(
            db,
            "delivery_contact",
            {
                "source": m["source"],
                "id": m["id"],
                "contact": "latest@example.invalid",
                "verification_evidence": "Synthetic subsequent verified recipient",
            },
            uuid4(),
        )
        assert corrected["in_progress_attempts"] == 1, corrected
        assert await op(db, "ack_email", ack(admitted))
        assert await op(db, "ack_email", ack(admitted))
        assert not await op(db, "ack_email", ack(admitted, "uncertain"))
        assert await db.fetchval(
            "SELECT delivered_at IS NULL FROM moderation_delivery WHERE source=$1 AND id=$2", m["source"], UUID(m["id"])
        )
        assert (
            await db.fetchval(
                "SELECT status FROM moderation_send_outcomes WHERE attempt_id=$1", UUID(admitted["attempt_id"])
            )
            == "transport_accepted"
        )
        print(
            "PASS claim identifiers only; stale unadmitted correction fenced; bounded admitted result retained without marking new generation delivered; exact/conflicting acknowledgement",
            flush=True,
        )

        # A Challenges recovery email is transported by backend. Disposal follows the
        # owning case, and must not touch the same UUID in the backend namespace.
        case = uuid4()
        m = message(case)
        twin = message(case, source="backend")
        for row in [m, twin]:
            await op(db, "accept_delivery", row)
        await op(
            db,
            "recovery_request",
            {
                "source": "challenges",
                "case_id": str(case),
                "contact": m["contact"],
                "hash": uuid4().hex + uuid4().hex,
                "link": "https://example.invalid/synthetic-private-capability",
                "ip_hash": uuid4().hex,
            },
        )
        recovery = await db.fetchrow(
            "SELECT id FROM moderation_delivery WHERE case_id=$1 AND audience='recovery'", case
        )
        rows = await op(db, "claim_email")
        q = next(r for r in rows if r["id"] == str(recovery["id"]))
        a = await op(db, "admit_email", q)
        assert a and a["source"] == "backend"
        t = await op(db, "admit_email", next(r for r in rows if r["id"] == twin["id"]))
        assert t
        # Pause an actual acknowledgement at its owning-case lock, after its routing
        # read. Disposal commits first; the acknowledgement must re-read the archive.
        tx = db.transaction()
        await tx.start()
        await db.execute(
            "SELECT pg_advisory_xact_lock(hashtextextended('external-disposal:challenges:'||$1::uuid,0))", case
        )
        pending = asyncio.create_task(op(other, "ack_email", ack(a)))
        await asyncio.sleep(0.15)
        assert not pending.done()
        await op(db, "accept_disposal", {"source": "challenges", "case_id": str(case)})
        await tx.commit()
        assert await pending
        assert await op(db, "ack_email", ack(a))
        assert not await op(db, "ack_email", ack(a, "uncertain"))
        assert await op(db, "ack_email", ack(t))
        tomb = await db.fetchval(
            "SELECT transport_evidence::text FROM moderation_external_disposals WHERE source='challenges' AND case_id=$1",
            case,
        )
        assert "destination_digest" not in tomb and "body_digest" not in tomb and "example.invalid" not in tomb
        assert not await db.fetchval(
            "SELECT EXISTS(SELECT 1 FROM moderation_send_attempts WHERE authority_source='challenges' AND case_id=$1)",
            case,
        )
        assert await db.fetchval(
            "SELECT EXISTS(SELECT 1 FROM moderation_send_attempts WHERE authority_source='backend' AND case_id=$1)",
            case,
        )
        assert await db.fetchval(
            "SELECT EXISTS(SELECT 1 FROM moderation_delivery WHERE source='backend' AND id=$1)", UUID(twin["id"])
        )
        assert not await db.fetchval(
            "SELECT EXISTS(SELECT 1 FROM moderation_delivery WHERE case_id=$1 AND (source='challenges' OR body->>'case_source'='challenges'))",
            case,
        )
        print(
            "PASS admitted cross-service recovery disposal/late acknowledgement race; unknown admission and actual outcome remain distinct; exact/conflicting replay and same UUID namespace isolation; no live contact/digest residue",
            flush=True,
        )

        # First adoption and account email updates serialize even with no contact row.
        person = await db.fetchval("SELECT id FROM users WHERE name='bar'")
        assert person
        await db.execute("UPDATE users SET email='first-a@example.invalid',email_verified=true WHERE id=$1", person)
        m = message(recipient=person, audience="author")
        tx = db.transaction()
        await tx.start()
        await op(db, "accept_delivery", m)
        change = asyncio.create_task(
            other.execute("UPDATE users SET email='first-b@example.invalid',email_verified=true WHERE id=$1", person)
        )
        await asyncio.sleep(0.15)
        assert not change.done()
        await tx.commit()
        await change
        assert (
            await db.fetchval("SELECT contact FROM moderation_recipient_contacts WHERE case_id=$1", UUID(m["case_id"]))
            == "first-b@example.invalid"
        )
        # Reverse commit order: address and verification are the same user generation.
        m2 = message(recipient=person, audience="author")
        tx = db.transaction()
        await tx.start()
        await db.execute("UPDATE users SET email='first-c@example.invalid',email_verified=false WHERE id=$1", person)
        adoption = asyncio.create_task(op(other, "accept_delivery", m2))
        await asyncio.sleep(0.15)
        assert not adoption.done()
        await tx.commit()
        await adoption
        assert await db.fetchval(
            "SELECT contact IS NULL AND authority_kind='unknown' FROM moderation_recipient_contacts WHERE case_id=$1",
            UUID(m2["case_id"]),
        )
        await op(
            db,
            "accept_minimization",
            {"source": "challenges", "case_id": m["case_id"], "id": str(uuid4()), "field": "author_contact"},
        )
        await db.execute("UPDATE users SET email='first-d@example.invalid',email_verified=true WHERE id=$1", person)
        await op(db, "accept_delivery", m | {"id": str(uuid4())})
        assert await db.fetchval(
            "SELECT contact IS NULL AND authority_kind='minimized' FROM moderation_recipient_contacts WHERE case_id=$1",
            UUID(m["case_id"]),
        )
        assert not await db.fetchval(
            "SELECT EXISTS(SELECT 1 FROM moderation_delivery WHERE case_id=$1 AND (contact IS NOT NULL OR owner_contact IS NOT NULL))",
            UUID(m["case_id"]),
        )
        print(
            "PASS first-adoption/account-update both orders; unverified Boolean cannot bless old address; reviewed minimization survives ordinary verified update and new owner message",
            flush=True,
        )
    finally:
        await other.close()
        await db.close()


if __name__ == "__main__":
    asyncio.run(main())
