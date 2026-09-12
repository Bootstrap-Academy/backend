"""Real PostgreSQL regression for historical moderation email policy.

Creates and stops its own private Unix-socket-only cluster; never uses a DSN,
existing database, SMTP service or user data. Requires PostgreSQL tools on PATH
(or --pg-bin) and the paired reviewed Challenges checkout via --challenges.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor
import difflib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
from uuid import uuid4

ROOT = Path(__file__).resolve().parents[1]
MIGRATION = "2026-09-12-070000_legacy_moderation_email"
ZERO = "00000000-0000-0000-0000-000000000000"


def literal(value):
    return "'" + str(value).replace("'", "''") + "'"


def json_sql(value):
    return literal(json.dumps(value)) + "::jsonb"


def function(text, name):
    match = re.search(r"CREATE (?:OR REPLACE )?FUNCTION " + name + r"\(.*?\$\$;", text, re.S)
    assert match, name
    return match.group().replace("CREATE FUNCTION", "CREATE OR REPLACE FUNCTION", 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pg-bin", type=Path)
    parser.add_argument("--challenges", type=Path, required=True)
    parser.add_argument("--evidence-dir", type=Path)
    parser.add_argument("--old-challenges-migrator", type=Path)
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error("run as an unprivileged user")
    found = shutil.which("pg_ctl")
    if args.pg_bin is None and found is None:
        parser.error("PostgreSQL tools required")
    pg = args.pg_bin.resolve() if args.pg_bin else Path(found).resolve().parent
    challenges = args.challenges.resolve()
    env = {k: v for k, v in os.environ.items() if not k.startswith("PG")}
    env["LC_ALL"] = "C"
    role = "moderation_email_test"
    checks = []
    function_diffs = {}
    with tempfile.TemporaryDirectory(prefix="academy-legacy-email-") as directory:
        work = Path(directory)
        work.chmod(0o700)
        data, socket = work / "data", work / "socket"
        socket.mkdir(mode=0o700)
        started = False

        def sql(db, statement, fail=False):
            result = subprocess.run(
                [str(pg / "psql"), "-XAt", "-v", "ON_ERROR_STOP=1", "-h", str(socket), "-U", role, "-d", db],
                input=statement,
                text=True,
                capture_output=True,
                env=env,
                timeout=60,
            )
            if fail:
                assert result.returncode, "unexpected SQL success"
            else:
                assert result.returncode == 0, result.stderr
            return result.stdout.strip()

        def value(db, statement):
            return json.loads(sql(db, "SELECT (" + statement + ")::text;"))

        def op(name, body=None):
            return value("backend", f"backend_moderation({literal(name)},NULL,{json_sql(body or {})})")

        def snapshot(db, tables):
            return {
                table: value(
                    db, f"(SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]') FROM {table} t)"
                )
                for table in tables
            }

        def make_message(outcome="legacy_observed"):
            return {
                "source": "challenges",
                "id": str(uuid4()),
                "case_id": str(uuid4()),
                "recipient": str(uuid4()),
                "audience": "notifier",
                "body": {"decision_id": str(uuid4()), "outcome": outcome, "rationale": "Synthetic message"},
                "contact": "fixture@example.invalid",
                "available_at": "2026-09-01T00:00:00Z",
            }

        def accept(message):
            assert op("accept_delivery", message)

        def ack(admitted, status):
            assert op(
                "ack_email", {k: admitted[k] for k in ["source", "id", "generation", "attempt_id"]} | {"status": status}
            )

        try:
            subprocess.run(
                [str(pg / "initdb"), "-D", str(data), "-U", role, "--auth=trust", "--no-locale", "--encoding=UTF8"],
                check=True,
                capture_output=True,
                env=env,
                timeout=60,
            )
            with (data / "postgresql.conf").open("a") as config:
                config.write(
                    f"\nlisten_addresses = ''\nunix_socket_directories = '{socket}'\nstatement_timeout = '45s'\n"
                )
            subprocess.run(
                [str(pg / "pg_ctl"), "-D", str(data), "-l", str(work / "postgres.log"), "-w", "start"],
                check=True,
                capture_output=True,
                env=env,
                timeout=60,
            )
            started = True
            sql("postgres", "CREATE DATABASE backend; CREATE DATABASE challenges;")
            migrations = ROOT / "academy_persistence/postgres/migrations"
            for migration in sorted(migrations.iterdir()):
                if migration.name < MIGRATION:
                    sql("backend", "BEGIN;\n" + (migration / "up.sql").read_text() + "\nCOMMIT;")

            # Actual pre-fix claim/admission: prove migration preserves prior
            # transport evidence and fences a claim already held by an old worker.
            historic = [make_message() for _ in range(5)]
            historic[0]["source"] = "backend"
            for message in historic:
                accept(message)
            claims = {row["id"]: row for row in op("claim_email")}
            ack(op("admit_email", claims[historic[2]["id"]]), "transport_accepted")
            ack(op("admit_email", claims[historic[3]["id"]]), "uncertain")
            sql(
                "backend",
                f"UPDATE moderation_delivery SET status='no_contact',lease_until=NULL WHERE id={literal(historic[4]['id'])};",
            )
            tables = [
                "moderation_delivery",
                "moderation_delivery_events",
                "moderation_send_attempts",
                "moderation_send_outcomes",
            ]
            before = snapshot("backend", tables)
            signatures = [
                "backend_moderation(text,uuid,jsonb)",
                "backend_moderation_before_invoice_identity(text,uuid,jsonb)",
                "moderation_admit_email(text,uuid,bigint)",
                "moderation_claim(integer)",
                "moderation_decide(uuid,jsonb)",
            ]
            functions_before = {
                name: sql("backend", f"SELECT pg_get_functiondef({literal(name)}::regprocedure);")
                for name in signatures
            }
            sql("backend", "BEGIN;\n" + (migrations / MIGRATION / "up.sql").read_text() + "\nCOMMIT;")
            assert snapshot("backend", tables) == before
            functions_after = {
                name: sql("backend", f"SELECT pg_get_functiondef({literal(name)}::regprocedure);")
                for name in signatures
            }
            assert functions_before[signatures[0]] == functions_after[signatures[0]], "invoice identity wrapper changed"
            assert op("retained_records")["invoice_identity_scope"].startswith(
                "Known pending invoice contents are withheld"
            )
            for name in signatures:
                function_diffs[name] = "".join(
                    difflib.unified_diff(
                        functions_before[name].splitlines(True),
                        functions_after[name].splitlines(True),
                        fromfile="deployed/" + name,
                        tofile="candidate/" + name,
                    )
                )
            checks.append(
                "latest invoice-identity dispatcher preserved byte-for-byte; actual migrated PostgreSQL function deltas recorded"
            )
            assert op("admit_email", claims[historic[0]["id"]]) is None
            assert op("admit_email", claims[historic[1]["id"]]) is None
            sql(
                "backend",
                "UPDATE moderation_delivery SET lease_until=clock_timestamp()-interval '1 second',next_attempt_at=clock_timestamp()-interval '1 second' WHERE delivered_at IS NULL AND status<>'no_contact';",
            )
            unchanged = snapshot("backend", tables)
            assert op("claim_email") == []
            assert snapshot("backend", tables) == unchanged
            checks.append(
                "migration preserves all prior transport rows; old claimed/pending/uncertain/no_contact legacy cannot be admitted or reclaimed"
            )

            # Unknown/current-message envelopes, routine receipts and notifier
            # updates never acquire email authority. Adoption still succeeds.
            routine = []
            for audience in ["author", "notifier"]:
                for body in [
                    {"status": "received"},
                    {"status": "complaint_received"},
                    {"outcome": None},
                    {"outcome": "warn"},
                    {"outcome": "restore"},
                    {"outcome": {"outcome": "legacy_observed"}},
                ]:
                    message = make_message() | {"audience": audience, "body": body}
                    routine.append(message)
                    accept(message)
                    accept(
                        message
                        | {"email_policy": {"channel": "email", "basis": "current_message", "decision_id": None}}
                    )
            assert op("claim_email") == []
            checks.append(
                "default denies unknown/missing/old permissive policy, routine report/complaint/warn and notifier updates; every message remains adopted"
            )

            def user(person=None):
                person = person or str(uuid4())
                email = person + "@example.invalid"
                name = "Fixture-" + person[:8]
                sql(
                    "backend",
                    f"BEGIN; INSERT INTO users(id,name,email,email_verified,created_at,enabled,admin) VALUES({literal(person)},{literal(name)},{literal(email)},true,clock_timestamp(),true,false); INSERT INTO user_profiles(user_id,display_name,bio,tags) VALUES({literal(person)},'Synthetic profile','',ARRAY[]::text[]); INSERT INTO user_invoice_info(user_id) VALUES({literal(person)}); COMMIT;",
                )
                return person, email, name

            # Exact shared owner SQL plus actual Challenges target adapters.
            initial = (challenges / "migration/src/moderation.sql").read_text()
            sql("challenges", initial)
            sql("challenges", (challenges / "migration/src/moderation_review_corrections.sql").read_text())
            sql("challenges", (challenges / "migration/src/moderation_retained_work.sql").read_text())
            adapter = (challenges / "migration/src/moderation_challenges.sql").read_text()
            sql(
                "challenges",
                "CREATE TYPE challenges_ban_action AS ENUM ('create','report'); CREATE TABLE challenges_ban(id uuid PRIMARY KEY,user_id uuid,start timestamp,\"end\" timestamp,action challenges_ban_action,creator uuid,reason text,rescinded boolean); CREATE TABLE challenges_subtasks(id uuid PRIMARY KEY,creator uuid,enabled boolean,retired boolean,moderation_removed boolean,task_id uuid); CREATE TABLE challenges_challenges(task_id uuid PRIMARY KEY,title text);",
            )
            for name in [
                "moderation_lock_target",
                "moderation_project",
                "moderation_adopt_target",
                "moderation_pending_work",
            ]:
                sql("challenges", function(adapter, name))
            common = (challenges / "migration/src/moderation_legacy_email_policy.sql").read_text()
            for name in [
                "moderation_decide",
                "moderation_important_email_decision",
                "moderation_message_email_policy",
                "moderation_message_email_context",
                "moderation_claim",
            ]:
                assert function((migrations / MIGRATION / "up.sql").read_text(), name) == function(common, name)
            sql("challenges", common)
            checks.append(
                "both owners have byte-identical native decision capture, importance classifier, policy, context and relay functions"
            )

            scopes = {
                "account": "Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten",
                "create": "Erstellen von Teilaufgaben auf Bootstrap Academy",
                "report": "Melden von Teilaufgaben auf Bootstrap Academy",
                "subtask": "Diese Teilaufgabe auf Bootstrap Academy",
            }

            def open_case(owner, kind, source="own_review", title=None):
                person, email, name = user()
                case, notifier, human = (str(uuid4()) for _ in range(3))
                target = str(uuid4()) if kind == "subtask" else person
                sql(
                    owner,
                    f"INSERT INTO moderation_targets(kind,id,subject) VALUES({literal(kind)},{literal(target)},{literal(person)});",
                )
                if kind == "subtask":
                    task = str(uuid4())
                    sql(
                        owner,
                        f"INSERT INTO challenges_subtasks(id,task_id,creator,enabled,retired,moderation_removed) VALUES({literal(target)},{literal(task)},{literal(person)},true,false,false);",
                    )
                    if title:
                        sql(
                            owner,
                            f"INSERT INTO challenges_challenges(task_id,title) VALUES({literal(task)},{literal(title)});",
                        )
                evidence = {
                    "author_contact": email,
                    "notifier_contact": "notifier@example.invalid",
                    "private_comment": "Never email this evidence",
                }
                if source == "legacy_import":
                    sql(
                        owner,
                        f"INSERT INTO moderation_cases(id,target_kind,target_id,subject,source,notifier,private_evidence) VALUES({literal(case)},{literal(kind)},{literal(target)},{literal(person)},'legacy_import',{literal(notifier)},{json_sql(evidence)});",
                    )
                else:
                    if source == "authority_order":
                        evidence.update(
                            authority="Synthetic authority",
                            order_reference="Synthetic order",
                            notification_instructions="Synthetic valid instruction",
                        )
                    sql(
                        owner,
                        f"SELECT moderation_open({literal(case)},{literal(human)},{literal(kind)},{literal(target)},{literal(person)},{literal(source)},{literal(notifier)},{json_sql(evidence)});",
                    )
                return {
                    "owner": owner,
                    "kind": kind,
                    "case": case,
                    "person": person,
                    "email": email,
                    "name": name,
                    "target": target,
                    "notifier": notifier,
                    "human": human,
                    "title": title,
                }

            def decide(case, outcome, **extra):
                revision = value(
                    case["owner"], f"(SELECT revision FROM moderation_cases WHERE id={literal(case['case'])})"
                )
                request = {
                    "request_key": str(uuid4()),
                    "case_id": case["case"],
                    "expected_revision": revision,
                    "reviewed_content_revision": 0,
                    "outcome": outcome,
                    "rationale": "Synthetic actual assessment",
                    "ground": "Synthetic grounds",
                    "rule_version": "Synthetic rules",
                    "automation": "Fixture",
                    "scope": scopes[case["kind"]],
                    "redress": "Human review available",
                    "misconduct_facts": "Synthetic facts",
                    "proportionality": "Synthetic assessment",
                    "hearing": "Synthetic hearing",
                    "duration_policy": "individual_assessment",
                    "duration_reason": "Synthetic duration",
                    "prior_warning": "Synthetic warning",
                    "absolute_frequency": "1",
                    "relative_frequency": "1 of 10",
                    "seriousness": "Synthetic seriousness",
                    "intent_assessment": "Synthetic assessment",
                    "order_event_evidence": "Synthetic actual instruction",
                    "ends_at": "2099-01-01T00:00:00Z",
                    # A caller's purported importance or policy is ignored.
                    "important": True,
                    "email_policy": {"channel": "email", "basis": "important_change"},
                }
                request.update(extra)
                actor = ZERO if outcome == "provisional" else case["human"]
                return value(case["owner"], f"moderation_decide({literal(actor)},{json_sql(request)})")

            def relay(case, expect_email, *, admit=True):
                messages = [m for m in value(case["owner"], "moderation_claim(50)") if m["case_id"] == case["case"]]
                expected = []
                for message in messages:
                    should_mail = (
                        expect_email and message["audience"] == "author" and message["body"].get("decision_id")
                    )
                    assert message["email_policy"]["channel"] == ("email" if should_mail else "inbox_only"), message[
                        "email_policy"
                    ]
                    if should_mail:
                        assert message["email_policy"]["basis"] == "important_change"
                        expected.append(message)
                    accept(message | {"source": case["owner"]})
                    assert value(
                        case["owner"], f"moderation_ack({literal(message['id'])},{message['generation']},true)"
                    )
                claims = op("claim_email")
                assert {r["id"] for r in claims} == {m["id"] for m in expected}, (claims, expected)
                if admit:
                    for claim in claims:
                        admitted = op("admit_email", claim)
                        assert admitted and admitted["audience"] == "author"
                        assert admitted["recipient_name"] == case["name"]
                        assert admitted["email_context"]["target_kind"] == case["kind"]
                        assert admitted["email_context"].get("target_title") == case["title"]
                        ack(admitted, "transport_accepted")
                return messages, claims

            # Every normal receipt and warning stays inside the product; native
            # before/after changes authorise exactly one affected-author notice.
            for owner, kind in [
                ("backend", "account"),
                ("challenges", "create"),
                ("challenges", "report"),
                ("challenges", "subtask"),
            ]:
                case = open_case(owner, kind, title="Synthetic parent challenge" if kind == "subtask" else None)
                relay(case, False)  # received
                decide(case, "warn")
                relay(case, False)
                restrict = "provisional" if kind == "subtask" else "restrict"
                first = decide(case, restrict)
                messages, _ = relay(case, True)
                assert first["effect_before"]["enabled"] and not first["effective"]["enabled"]
                assert set(first["effect_before"]) == {"enabled", "removed", "retired", "withdrawn"}
                assert set(first["measure_after"]) == {"effect", "active", "rescinded", "ends_at"}
                stored_request = value(
                    owner, f"(SELECT request FROM moderation_decisions WHERE id={literal(first['decision_id'])})"
                )
                before_replay = snapshot(owner, ["moderation_decisions", "moderation_messages"])
                actor = ZERO if restrict == "provisional" else case["human"]
                assert value(owner, f"moderation_decide({literal(actor)},{json_sql(stored_request)})") == first
                assert snapshot(owner, ["moderation_decisions", "moderation_messages"]) == before_replay
                if kind == "subtask":
                    # Another already active case masks this new provisional
                    # hold: no new target effect, so no redundant author email.
                    other = case | {"case": str(uuid4()), "notifier": str(uuid4())}
                    evidence = {"author_contact": case["email"], "notifier_contact": "other@example.invalid"}
                    sql(
                        owner,
                        f"SELECT moderation_open({literal(other['case'])},{literal(case['human'])},'subtask',{literal(case['target'])},{literal(case['person'])},'own_review',{literal(other['notifier'])},{json_sql(evidence)});",
                    )
                    relay(other, False)
                    redundant = decide(other, "provisional")
                    relay(other, False)
                    assert case["case"] not in json.dumps(redundant)
                    assert redundant["effect_before"] == redundant["effective"]
                author = next(m for m in messages if m["audience"] == "author")
                assert author["email_context"]["decision_automatic"] == (kind == "subtask")
                original_context = author["email_context"]
                accept(
                    author
                    | {"source": owner, "email_context": {"target_kind": kind, "target_title": "Later renamed parent"}}
                )
                assert (
                    value(
                        "backend",
                        f"(SELECT evidence FROM moderation_delivery_events WHERE source={literal(owner)} AND message_id={literal(author['id'])} AND event='owner_email_context')",
                    )
                    == original_context
                )
                # Identical restriction and unchanged uphold must not make mail.
                decide(case, restrict)
                relay(case, False)
                decide(case, "uphold", ends_at="2099-03-01T00:00:00Z")
                relay(case, False)
                # A real duration change matters despite an unchanged enabled flag.
                decide(case, restrict, ends_at="2099-02-01T00:00:00Z")
                relay(case, True)
                decide(case, "restore", ends_at=None)
                relay(case, True)
                decide(case, "restore", ends_at=None)
                relay(case, False)
                assert (
                    value(owner, f"(SELECT count(*) FROM moderation_messages WHERE case_id={literal(case['case'])})")
                    == 15
                )
                assert len(value(owner, f"moderation_inbox({literal(case['notifier'])})")) == 8
                assert value(
                    owner,
                    f"(SELECT bool_and(informed_at IS NULL AND notification_evidence IS NULL) FROM moderation_messages WHERE case_id={literal(case['case'])} AND audience='notifier')",
                )
                checks.append(
                    f"{owner}/{kind}: receipts/warn/repeated restrict/unchanged uphold/repeated restore blocked; actual restriction/duration change/release reaches only author; inbox and context retained"
                )

            authority = open_case("challenges", "create", source="authority_order")
            relay(authority, False)
            decide(authority, "authority_start")
            relay(authority, True)
            decide(authority, "authority_change")
            relay(authority, False)
            decide(authority, "authority_change", ends_at="2099-02-01T00:00:00Z")
            relay(authority, True)
            decide(authority, "authority_end", ends_at=None)
            relay(authority, True)
            checks.append(
                "unchanged authority_change is inbox-only; real authority duration change and end remain important"
            )

            # Exercise the actual native decision across an independent hold's
            # wall-clock expiry. A fixture wrapper captures the real first effect,
            # waits until the separate deadline has passed, then lets subsequent
            # calls use the real SQL unchanged. No other case IDs reach the body.
            for owner in ["backend", "challenges"]:
                original_effect = sql(owner, "SELECT pg_get_functiondef('moderation_effect(text,uuid)'::regprocedure);")
                sql(
                    owner,
                    original_effect.replace(
                        "FUNCTION public.moderation_effect(", "FUNCTION public.fixture_real_effect("
                    ),
                )
                sql(
                    owner,
                    """
                    CREATE TABLE fixture_clock_boundary(case_id uuid, target_id uuid, armed boolean);
                    CREATE OR REPLACE FUNCTION moderation_effect(p_kind text,p_id uuid) RETURNS jsonb
                    LANGUAGE plpgsql VOLATILE AS $$
                    DECLARE result jsonb; boundary uuid; deadline timestamptz;
                    BEGIN
                      SELECT case_id INTO boundary FROM fixture_clock_boundary WHERE target_id=p_id AND armed;
                      IF FOUND THEN
                        UPDATE fixture_clock_boundary SET armed=false WHERE case_id=boundary;
                        UPDATE moderation_holds SET ends_at=clock_timestamp()+interval '30 milliseconds'
                          WHERE case_id=boundary RETURNING ends_at INTO deadline;
                        result:=fixture_real_effect(p_kind,p_id);
                        PERFORM pg_sleep(greatest(0,extract(epoch FROM deadline-clock_timestamp()))+0.01);
                        RETURN result;
                      END IF;
                      RETURN fixture_real_effect(p_kind,p_id);
                    END $$;
                    """,
                )
                scenarios = (
                    [("account", "warn", "uphold")]
                    if owner == "backend"
                    else [
                        ("create", "restrict", "restrict"),
                        ("subtask", "provisional", "provisional"),
                        ("subtask", "authority_start", "authority_change"),
                    ]
                )
                for kind, initial, repeated in scenarios:
                    source = "authority_order" if initial == "authority_start" else "own_review"
                    case = open_case(owner, kind, source=source)
                    relay(case, False)
                    decide(case, initial)
                    relay(case, initial != "warn")
                    other = case | {"case": str(uuid4()), "notifier": str(uuid4())}
                    sql(
                        owner,
                        f"SELECT moderation_open({literal(other['case'])},{literal(case['human'])},{literal(kind)},{literal(case['target'])},{literal(case['person'])},'own_review',{literal(other['notifier'])},'{{}}');",
                    )
                    relay(other, False)
                    effect = "retire" if kind == "subtask" else "restrict"
                    sql(
                        owner,
                        f"INSERT INTO moderation_holds(case_id,target_kind,target_id,effect,starts_at,ends_at) VALUES({literal(other['case'])},{literal(kind)},{literal(case['target'])},{literal(effect)},clock_timestamp()-interval '1 day',clock_timestamp()+interval '1 day'); INSERT INTO fixture_clock_boundary VALUES({literal(other['case'])},{literal(case['target'])},true);",
                    )
                    statement = decide(case, repeated)
                    assert statement["measure_before"] == statement["measure_after"]
                    if repeated != "restrict":
                        assert statement["effect_before"] != statement["effective"]
                    assert other["case"] not in json.dumps(statement)
                    relay(case, False)
                    # This synthetic standalone timing hold has no native
                    # decision for later maintenance; disarm it with the clock fixture.
                    sql(owner, f"UPDATE moderation_holds SET active=false WHERE case_id={literal(other['case'])};")
                sql(
                    owner,
                    original_effect
                    + "; DROP FUNCTION fixture_real_effect(text,uuid); DROP TABLE fixture_clock_boundary;",
                )
            checks.append(
                "real independent clock expiry cannot authorise unchanged uphold, restrict, provisional or authority_change; own measure comparison gates global differences"
            )

            for owner, kind in [("backend", "account"), ("challenges", "create"), ("challenges", "report")]:
                case = open_case(owner, kind, source="legacy_import")
                sql(
                    owner,
                    f"INSERT INTO moderation_holds(case_id,target_kind,target_id,effect,starts_at,ends_at) VALUES({literal(case['case'])},{literal(kind)},{literal(case['target'])},'restrict',clock_timestamp()-interval '2 days',clock_timestamp()-interval '1 day'); SELECT moderation_legacy_statements(); SELECT moderation_maintenance();",
                )
                relay(case, False)
                decide(case, "restore", ends_at=None)
                relay(case, False)  # Merely re-stating an already ended old measure.
                decide(case, "restrict")
                relay(case, True)
                sql(
                    owner,
                    f"UPDATE moderation_holds SET ends_at=clock_timestamp()-interval '1 second' WHERE case_id={literal(case['case'])}; SELECT moderation_maintenance();",
                )
                relay(case, True)
                checks.append(
                    f"{owner}/{kind}: import/initial expiry/redundant historical restore denied, subsequent genuinely new restriction and its expiry admitted"
                )

            # Suppression survives an actual verified-account contact change and
            # verified contact correction; neither means a message is important.
            quiet = open_case("challenges", "create")
            relay(quiet, False)
            decide(quiet, "warn")
            messages, _ = relay(quiet, False)
            quiet_author = next(m for m in messages if m["audience"] == "author")
            sql("backend", f"UPDATE users SET email='changed@example.invalid' WHERE id={literal(quiet['person'])};")
            body = {
                "source": "challenges",
                "id": quiet_author["id"],
                "contact": "corrected@example.invalid",
                "verification_evidence": "Synthetic actual contact verification",
            }
            value("backend", f"backend_moderation('delivery_contact',{literal(uuid4())},{json_sql(body)})")
            assert op("claim_email") == []
            assert value(
                "backend",
                f"(SELECT delivered_at IS NULL FROM moderation_delivery WHERE id={literal(quiet_author['id'])})",
            )
            checks.append(
                "account/contact corrections do not reactivate suppressed routine messages or fabricate delivery"
            )

            # A copied old envelope cannot gain authority merely from an outcome
            # or an old permissive policy. Explicit user-requested recovery is separate.
            recovery_source = make_message() | {"body": {"status": "received"}}
            accept(recovery_source)
            request = {
                "source": "challenges",
                "case_id": recovery_source["case_id"],
                "contact": recovery_source["contact"],
                "hash": uuid4().hex + uuid4().hex,
                "link": "https://example.invalid/explicitly-requested-access",
                "ip_hash": uuid4().hex,
            }
            assert op("recovery_request", request)
            recovery_claim = op("claim_email")
            assert len(recovery_claim) == 1
            recovery = op("admit_email", recovery_claim[0])
            assert recovery and recovery["audience"] == "recovery"
            ack(recovery, "transport_accepted")
            # An adopted message mimicking recovery never has native recovery provenance.
            imitation = make_message() | {
                "source": "backend",
                "case_id": recovery["case_id"],
                "recipient": recovery["recipient"],
                "audience": "recovery",
                "body": recovery["body"],
            }
            accept(imitation)
            assert op("claim_email") == []
            second_source = make_message() | {"body": {"status": "received"}}
            accept(second_source)
            request.update(case_id=second_source["case_id"], hash=uuid4().hex + uuid4().hex, ip_hash=uuid4().hex)
            assert op("recovery_request", request)
            claim = op("claim_email")
            assert len(claim) == 1
            sql(
                "backend",
                f"UPDATE moderation_capabilities SET revoked_at=clock_timestamp() WHERE hash={literal(request['hash'])};",
            )
            assert op("admit_email", claim[0]) is None
            checks.append(
                "only actual native requested recovery with valid capability is admitted; imitation and revocation are denied"
            )

            # An overlapping owner suppression must be visible to final admission.
            racing = open_case("challenges", "create")
            relay(racing, False)
            decide(racing, "restrict")
            messages, claims = relay(racing, True, admit=False)
            message = next(m for m in messages if m["audience"] == "author") | {"source": "challenges"}
            locker = subprocess.Popen(
                [str(pg / "psql"), "-XAt", "-v", "ON_ERROR_STOP=1", "-h", str(socket), "-U", role, "-d", "backend"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                env=env,
                bufsize=1,
            )
            try:
                locker.stdin.write(
                    f"BEGIN; SELECT pg_advisory_xact_lock(hashtextextended('external-disposal:challenges:'||{literal(racing['case'])},0)); SELECT 'lock_ready';\n"
                )
                locker.stdin.flush()
                while locker.stdout.readline().strip() != "lock_ready":
                    assert locker.poll() is None
                with ThreadPoolExecutor(max_workers=1) as executor:
                    waiting = executor.submit(op, "admit_email", claims[0])
                    deadline = time.monotonic() + 5
                    while not value(
                        "backend",
                        "(SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname='backend' AND wait_event='advisory' AND query LIKE '%admit_email%'))",
                    ):
                        assert time.monotonic() < deadline and not waiting.done()
                        time.sleep(0.02)
                    message["email_policy"] = {
                        "channel": "inbox_only",
                        "basis": "routine_or_unknown",
                        "decision_id": message["body"]["decision_id"],
                    }
                    locker.stdin.write(
                        f"SELECT backend_moderation('accept_delivery',NULL,{json_sql(message)}); COMMIT;\n"
                    )
                    locker.stdin.flush()
                    assert waiting.result(timeout=5) is None
                locker.stdin.close()
                assert locker.wait(timeout=5) == 0, locker.stderr.read()
            finally:
                if locker.poll() is None:
                    locker.terminate()
                    locker.wait(timeout=5)
            for invalid in [
                None,
                {},
                {"channel": "bogus"},
                {"channel": "email", "basis": "important_change", "decision_id": str(uuid4())},
            ]:
                sql(
                    "backend",
                    f"SELECT backend_moderation('accept_delivery',NULL,{json_sql(message | {'email_policy': invalid})});",
                    fail=True,
                )
            for invalid in [
                {"target_kind": "other"},
                {"target_kind": "subtask", "target_title": "x" * 201},
                {"target_kind": "subtask", "target_title": "bad\ncontrol"},
                {"target_kind": "subtask", "private_comment": "extra"},
                {"target_kind": "subtask", "decision_automatic": "true"},
            ]:
                sql(
                    "backend",
                    f"SELECT backend_moderation('accept_delivery',NULL,{json_sql(message | {'email_context': invalid})});",
                    fail=True,
                )
            checks.append(
                "final admission observes overlapping suppression after lock; malformed policy/context rejected"
            )

            # Optional actual deployed SeaORM binary: its absent future file
            # must reject startup without deleting the applied migration marker.
            if args.old_challenges_migrator:
                sql("postgres", "CREATE DATABASE rollback_probe;")
                version = "m20260912_070000_legacy_moderation_email"
                sql(
                    "rollback_probe",
                    f"CREATE TABLE seaql_migrations(version varchar(255) PRIMARY KEY,applied_at bigint NOT NULL); INSERT INTO seaql_migrations VALUES({literal(version)},0);",
                )
                before = snapshot("rollback_probe", ["seaql_migrations"])
                migration_env = env | {"DATABASE_URL": f"postgresql://{role}@localhost/rollback_probe?host={socket}"}
                result = subprocess.run(
                    [str(args.old_challenges_migrator.resolve()), "up"],
                    cwd=work,
                    env=migration_env,
                    capture_output=True,
                    text=True,
                    timeout=20,
                )
                output = result.stdout + result.stderr
                assert (
                    result.returncode != 0
                    and version in output
                    and "has been applied but its file is missing" in output
                )
                assert snapshot("rollback_probe", ["seaql_migrations"]) == before
                assert value("rollback_probe", "(SELECT count(*)=1 FROM pg_tables WHERE schemaname='public')")
                checks.append(
                    "actual old Challenges migration binary rejects the newly applied version; marker and schema remain unchanged, so old closure is not a safe full rollback"
                )

            # SQL down must not silently reactivate historical email delivery.
            sql("backend", (migrations / MIGRATION / "down.sql").read_text(), fail=True)
            checks.append("destructive rollback refused; forward protection retained")
            result = {
                "result": "passed",
                "postgres": subprocess.check_output([str(pg / "postgres"), "--version"], text=True).strip(),
                "smtp_connections": 0,
                "checks": checks,
            }
            if args.evidence_dir:
                args.evidence_dir.mkdir(mode=0o700, parents=True, exist_ok=False)
                for name, content in [
                    ("result.json", json.dumps(result, indent=2) + "\n"),
                    ("function-diffs.json", json.dumps(function_diffs, indent=2) + "\n"),
                ]:
                    path = args.evidence_dir / name
                    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
                    with os.fdopen(descriptor, "w") as target:
                        target.write(content)
            print(json.dumps(result, indent=2))
        finally:
            if started:
                subprocess.run(
                    [str(pg / "pg_ctl"), "-D", str(data), "-m", "fast", "-w", "stop"],
                    check=True,
                    capture_output=True,
                    env=env,
                    timeout=60,
                )


if __name__ == "__main__":
    main()
