"""Close declarations only when both original and current contract evidence are absent."""

import json
import subprocess
import uuid

from utils import create_admin_account, create_verified_account, make_client

admin = make_client()
create_admin_account("operator", "operator@example.invalid", "test-password", admin)


def sql(query):
    return subprocess.check_output(
        ["sudo", "-u", "postgres", "psql", "-XAt", "-v", "ON_ERROR_STOP=1", "-d", "academy", "-c", query], text=True
    ).strip()


def fixture(case, kind="cancellation", observation=True):
    client = make_client()
    # An observation can outlive its account; reproduce that state without running account erasure here.
    user = (
        str(uuid.uuid4())
        if case == "erasedaccount"
        else create_verified_account(case, case + "@example.invalid", "test-password", client)["user"]["id"]
    )
    declaration_user = "NULL" if case == "erasedaccount" else f"'{user}'"
    if case == "premium":
        sql(
            f"INSERT INTO premium(id,user_id,since,until) VALUES('{uuid.uuid4()}','{user}',now()-interval '2 years',now()-interval '1 year')"
        )
    declaration = str(uuid.uuid4())
    sql(
        f"INSERT INTO contract_declarations(id,kind,received_at,name,email,user_id,contract,cancellation_type,details) "
        f"VALUES('{declaration}','{kind}',clock_timestamp(),'Synthetic declaration','{case}@example.invalid',{declaration_user},'premium',"
        + ("'extraordinary'" if kind == "cancellation" else "NULL")
        + ",'Synthetic test')"
    )
    if observation:
        sql(f"INSERT INTO contract_account_observation(declaration_id,user_id) VALUES('{declaration}','{user}')")
    return user, declaration


def close(declaration, **changes):
    return admin.patch(
        "/contracts/declarations/" + declaration,
        json={
            "action": "CLOSE_WITHOUT_CONTRACT",
            "identity_verified": False,
            "note": "Kein Vertrag am Konto",
            **changes,
        },
    )


def snapshot(user, declaration):
    return sql(
        f"SELECT jsonb_build_object('account',(SELECT to_jsonb(u) FROM users u WHERE id='{user}'),"
        f"'deliveries',(SELECT coalesce(jsonb_agg(to_jsonb(m)),'[]') FROM contract_delivery m WHERE declaration_id='{declaration}'))"
    )


for kind in ["withdrawal", "cancellation"]:
    user, declaration = fixture(kind, kind)
    before = snapshot(user, declaration)
    response = close(declaration)
    assert response.status_code == 200, response.text
    result = response.json()
    assert result["processed_at"] and result["effective_end"] is None
    assert result["processing_note"] == "Kein Vertrag am Konto"
    assert snapshot(user, declaration) == before, "Account and delivery evidence stay unchanged"
    evidence = json.loads(result["operational_evidence"])
    assert evidence["processing_action"]["action"] == "close_without_contract"
    assert evidence["processing_action"]["effective_end"] is None
    assert sql(f"SELECT count(*) FROM contract_declarations WHERE id='{declaration}' AND processed_at IS NULL") == "0"
    assert close(declaration).status_code == 409

# Every request-only field is rejected before any mutation, and a note is required.
for case, changes in [
    ("date", {"effective_end": "2026-12-01T00:00:00Z"}),
    ("verifieduser", {"verified_user_id": str(uuid.uuid4())}),
    ("agreementinput", {"renewal_agreement_id": str(uuid.uuid4())}),
    ("missingnote", {"note": None}),
    ("emptynote", {"note": "   "}),
]:
    user, declaration = fixture(case)
    expected_status = 422 if case == "emptynote" else 400
    assert close(declaration, **changes).status_code == expected_status
    assert sql(f"SELECT count(*) FROM contract_processing_actions WHERE declaration_id='{declaration}'") == "0"

# Evidence rows are seeded independently so each guard has its own counterexample.
for case in [
    "observedagreement",
    "purchase",
    "period",
    "operation",
    "schedule",
    "premium",
    "renewal",
    "subscription",
    "invoice",
    "noaccount",
    "erasedaccount",
]:
    user, declaration = fixture(case, observation=case not in ["observedagreement", "noaccount"])
    agreement, period = str(uuid.uuid4()), str(uuid.uuid4())
    if case == "observedagreement":
        sql(
            f"INSERT INTO contract_account_observation(declaration_id,user_id,agreement_id) VALUES('{declaration}','{user}','{agreement}')"
        )
    if case == "purchase":
        sql(
            f"INSERT INTO contract_purchase_observations(declaration_id,order_id,evidence) VALUES('{declaration}','{uuid.uuid4()}','{{}}')"
        )
    if case == "period":
        sql(
            f"INSERT INTO contract_period_observation(declaration_id,premium_id,since,until,source) VALUES('{declaration}','{period}',now()-interval '2 years',now()-interval '1 year','receipt')"
        )
    if case == "operation":
        sql(f"INSERT INTO contract_premium_operations VALUES('{declaration}',1,'{{}}')")
    if case in ["schedule", "renewal", "subscription"]:
        # An unrelated owner keeps schedule rejection independent of the account guard.
        owner = str(uuid.uuid4()) if case in ["schedule", "subscription"] else user
        sql(
            f"INSERT INTO premium_renewal_agreements(id,user_id,received_at,offer_id,monthly_price,recipient,document,terms_pdf,withdrawal_pdf) VALUES('{agreement}','{owner}',now(),'test',100,'synthetic@example.invalid','test','test','test')"
        )
    if case == "schedule":
        sql(
            f"INSERT INTO contract_cancellation_schedule(declaration_id,agreement_id,requested_end,established_recipient,completed_at) VALUES('{declaration}','{agreement}',now(),'synthetic@example.invalid',now())"
        )
    if case == "subscription":
        sql(f"INSERT INTO premium_subscriptions(user_id,plan,agreement_id) VALUES('{user}','monthly','{agreement}')")
    if case == "invoice":
        sql(
            f"INSERT INTO financial_documents(number,kind,user_id,issued_at) VALUES('synthetic','invoice','{user}',now()-interval '5 years')"
        )
    before = snapshot(user, declaration)
    response = close(declaration)
    assert response.status_code == 400, (case, response.text)
    assert "Vertrag" in response.json()["detail"] and "Kontozuordnung" in response.json()["detail"]
    assert sql(f"SELECT count(*) FROM contract_processing_actions WHERE declaration_id='{declaration}'") == "0"
    assert sql(f"SELECT count(*) FROM contract_declarations WHERE id='{declaration}' AND processed_at IS NULL") == "1"
    assert snapshot(user, declaration) == before

print("PASS: both declaration kinds, every guard, unchanged account/delivery, action evidence and repeat conflict")
