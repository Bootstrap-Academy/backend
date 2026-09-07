import os
import subprocess
import time
from datetime import datetime, timezone

from utils import (
    c,
    create_admin_account,
    create_verified_account,
    decode_mail_header,
    decode_mail_payload,
    fetch_mail,
    make_client,
)

DECLARATION_KEYS = {
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
    "effective_end",
    "processed_at",
    "processing_note",
}


def fetch_mails(count):
    """Fetch `count` mails and index them by their recipient."""
    mails = {}
    for _ in range(count):
        mail = fetch_mail()
        mails[mail["X-Original-To"]] = mail
    assert len(mails) == count
    return mails


# a verified account with an active premium membership
login = create_verified_account("dieter", "dieter@example.com", "supersecurepassword")
user_id = login["user"]["id"]
assert subprocess.getstatusoutput(f"academy admin coin add {user_id} 5000")[0] == 0

resp = c.post(
    "/shop/premium",
    json={"plan": "MONTHLY", "autopay": True, "withdrawal_consent": True, "withdrawal_text_version": "2026-09"},
)
assert resp.status_code == 200
premium = resp.json()
assert premium["premium"] is True
assert premium["autopay"] == "MONTHLY"

# declare a cancellation
resp = c.post(
    "/contracts/cancellations",
    json={
        "name": "Dieter Mustermann",
        "email": "dieter@example.com",
        "contract": "PREMIUM",
        "contract_designation": "Premium-Abo, monatlich",
        "cancellation_type": "ORDINARY",
        "details": "Zu teuer",
        "requested_end": None,
    },
)
assert resp.status_code == 200
result = resp.json()
assert set(result) == {"declaration", "confirmation_email_sent"}
assert result["confirmation_email_sent"] is True

cancellation = result["declaration"]
assert set(cancellation) == DECLARATION_KEYS
assert cancellation["kind"] == "CANCELLATION"
assert cancellation["name"] == "Dieter Mustermann"
assert cancellation["email"] == "dieter@example.com"
assert cancellation["contract"] == "PREMIUM"
assert cancellation["contract_designation"] == "Premium-Abo, monatlich"
assert cancellation["cancellation_type"] == "ORDINARY"
assert cancellation["details"] == "Zu teuer"
assert cancellation["requested_end"] is None
assert cancellation["processed_at"] is None
assert cancellation["processing_note"] is None

# the contract ends when the paid period ends
until = datetime.fromtimestamp(premium["until"], timezone.utc).strftime("%Y-%m-%dT%H:%M:%S")
assert cancellation["effective_end"].startswith(until)

# the automatic renewal has been switched off, premium remains usable until then
status = c.get("/shop/premium/me").json()
assert status["premium"] is True
assert status["autopay"] is None

mails = fetch_mails(2)
assert set(mails) == {"dieter@example.com", "contact@academy"}

confirmation = mails["dieter@example.com"]
assert decode_mail_header(confirmation["Subject"]) == "Kündigungsbestätigung - Bootstrap Academy"
content = decode_mail_payload(confirmation)
assert "Wir bestätigen den Eingang Ihrer Kündigungserklärung." in content
assert "(Uhrzeit in der Zeitzone Europe/Berlin)" in content
assert "Begründung: Zu teuer" in content
assert "Ihre Bezeichnung des Vertrags: Premium-Abo, monatlich" in content
assert "Die automatische Verlängerung ist abgeschaltet" in content
assert "Diese Bestätigung erfolgt nach § 312k Abs. 4 BGB." in content

notification = mails["contact@academy"]
assert decode_mail_header(notification["Subject"]) == "[Contract] Kündigung (Premium)"
content = decode_mail_payload(notification)
assert "Art der Erklärung: Kündigung" in content
assert f"Konto: {user_id}" in content
assert "Vertrag: Premium-Mitgliedschaft" in content
assert "Bezeichnung laut Erklärung: Premium-Abo, monatlich" in content
assert "Art der Kündigung: ordentliche Kündigung" in content
assert f"ID der Erklärung: {cancellation['id']}" in content

# an extraordinary cancellation is not answered with the ordinary end date
resp = c.post(
    "/contracts/cancellations",
    json={
        "name": "Dieter Mustermann",
        "email": "dieter@example.com",
        "contract": "PREMIUM",
        "contract_designation": "Premium-Abo, monatlich",
        "cancellation_type": "EXTRAORDINARY",
        "details": "Leistung seit zwei Wochen nicht erreichbar",
    },
)
assert resp.status_code == 200
extraordinary = resp.json()["declaration"]
assert extraordinary["cancellation_type"] == "EXTRAORDINARY"
assert extraordinary["effective_end"] is None
assert extraordinary["processed_at"] is None

mails = fetch_mails(2)
assert set(mails) == {"dieter@example.com", "contact@academy"}

content = decode_mail_payload(mails["dieter@example.com"])
assert "Sie haben außerordentlich gekündigt." in content
assert "gesondert in Textform mit." in content
assert "Ihr Vertrag endet zum" not in content

notification = mails["contact@academy"]
assert decode_mail_header(notification["Subject"]) == "[Contract] DRINGEND: Kündigung (Premium)"
content = decode_mail_payload(notification)
assert "DRINGEND: außerordentliche Kündigung." in content
assert "Beendigungszeitpunkt: -" in content

# declare a withdrawal
resp = c.post(
    "/contracts/withdrawals",
    json={
        "name": "Dieter Mustermann",
        "email": "dieter@example.com",
        "contract": "COINS",
        "contract_designation": "MorphCoins, Bestellung 4711",
        "details": None,
    },
)
assert resp.status_code == 200
result = resp.json()
assert result["confirmation_email_sent"] is True

withdrawal = result["declaration"]
assert set(withdrawal) == DECLARATION_KEYS
assert withdrawal["kind"] == "WITHDRAWAL"
assert withdrawal["contract"] == "COINS"
assert withdrawal["contract_designation"] == "MorphCoins, Bestellung 4711"
assert withdrawal["cancellation_type"] is None
assert withdrawal["details"] is None
assert withdrawal["requested_end"] is None
assert withdrawal["effective_end"] is None
assert withdrawal["processed_at"] is None

mails = fetch_mails(2)
assert set(mails) == {"dieter@example.com", "contact@academy"}

confirmation = mails["dieter@example.com"]
assert decode_mail_header(confirmation["Subject"]) == "Widerrufsbestätigung - Bootstrap Academy"
content = decode_mail_payload(confirmation)
assert "Wir bestätigen den Eingang Ihrer Widerrufserklärung." in content
assert "Wir erstatten den gezahlten Betrag innerhalb von 14 Tagen über das ursprüngliche Zahlungsmittel." in content
assert "Diese Bestätigung erfolgt nach § 356a BGB." in content

notification = mails["contact@academy"]
assert decode_mail_header(notification["Subject"]) == "[Contract] Widerruf (Coins)"
content = decode_mail_payload(notification)
assert "Art der Erklärung: Widerruf" in content
assert "Vertrag: MorphCoins-Kauf" in content
assert "Bezeichnung laut Erklärung: MorphCoins, Bestellung 4711" in content

# the admin listing exposes the matched account
ca = make_client()
create_admin_account("admin", "admin@example.com", "supersecureadminpassword", ca)

resp = ca.get("/contracts/declarations")
assert resp.status_code == 200
listing = resp.json()
assert listing["total"] == 3
assert [d["kind"] for d in listing["declarations"]] == ["WITHDRAWAL", "CANCELLATION", "CANCELLATION"]
assert all(set(d) == DECLARATION_KEYS | {"user_id"} for d in listing["declarations"])
assert all(d["user_id"] == user_id for d in listing["declarations"])

resp = ca.get("/contracts/declarations", params={"kind": "CANCELLATION"})
assert resp.status_code == 200
listing = resp.json()
assert listing["total"] == 2
assert [d["id"] for d in listing["declarations"]] == [extraordinary["id"], cancellation["id"]]

resp = ca.get("/contracts/declarations", params={"limit": 1, "offset": 1})
assert resp.status_code == 200
listing = resp.json()
assert listing["total"] == 3
assert [d["id"] for d in listing["declarations"]] == [extraordinary["id"]]

# the extraordinary cancellation is answered by hand: an administrator records
# the end date that was confirmed in Textform and what was done
resp = ca.patch(
    f"/contracts/declarations/{extraordinary['id']}",
    json={"effective_end": "2026-10-31T22:59:59Z", "note": "Kündigung anerkannt, Ende bestätigt"},
)
assert resp.status_code == 200
processed = resp.json()
assert processed["id"] == extraordinary["id"]
assert processed["effective_end"].startswith("2026-10-31T22:59:59")
assert processed["processing_note"] == "Kündigung anerkannt, Ende bestätigt"
assert processed["processed_at"] is not None
assert processed["user_id"] == user_id

# the change is in the administrative audit log. Checked before the listing is
# read again, because reading the listing is recorded too.
log = ca.get("/admin/audit-log").json()
entry = log["entries"][0]
assert entry["method"] == "PATCH"
assert entry["path"] == f"/contracts/declarations/{extraordinary['id']}"
assert entry["status"] == 200

# and the listing shows the change
resp = ca.get("/contracts/declarations", params={"kind": "CANCELLATION"})
assert resp.json()["declarations"][0] == processed

# a field that is not given is left alone
resp = ca.patch(f"/contracts/declarations/{cancellation['id']}", json={})
assert resp.status_code == 200
assert resp.json()["effective_end"] == cancellation["effective_end"]
assert resp.json()["processing_note"] is None
assert resp.json()["processed_at"] is not None

resp = ca.patch("/contracts/declarations/0f5ba9d2-1a1c-4c22-bd2f-1cb0d9e0f8a1", json={})
assert resp.status_code == 404
assert resp.json() == {"detail": "Declaration not found"}

# recording the processing requires admin privileges, which since the audit log
# work means an mfa verified session
resp = c.patch(f"/contracts/declarations/{cancellation['id']}", json={})
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

# the admin listing requires admin privileges
resp = c.get("/contracts/declarations")
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

anonymous = make_client()
resp = anonymous.get("/contracts/declarations")
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid token"}

# the rate limit allows five declarations per hour and email address;
# dieter@example.com has used three of them above
for _ in range(2):
    resp = c.post(
        "/contracts/withdrawals",
        json={"name": "Dieter Mustermann", "email": "dieter@example.com", "contract": "OTHER", "details": None},
    )
    assert resp.status_code == 200, resp.text

resp = c.post(
    "/contracts/withdrawals",
    json={"name": "Dieter Mustermann", "email": "dieter@example.com", "contract": "OTHER", "details": None},
)
assert resp.status_code == 429
assert resp.json() == {"detail": "Too many requests"}

# the ip budget is much larger, so the next person behind the same address
# (a household, a school, a carrier-grade NAT) is not blocked by it
resp = c.post(
    "/contracts/withdrawals",
    json={"name": "Somebody Else", "email": "somebody@example.com", "contract": "OTHER", "details": None},
)
assert resp.status_code == 200, resp.text

# nothing has been stored for the rejected declaration
resp = ca.get("/contracts/declarations")
assert resp.status_code == 200
assert resp.json()["total"] == 6

# The declaration a person sends is not written into the log. The mail server
# is taken down so that the failure to send the confirmation is reported from
# inside the span of the request, which is where the fields of the enclosing
# spans are printed.
assert os.system("systemctl stop postfix.service") == 0
since = subprocess.check_output(["date", "+%Y-%m-%d %H:%M:%S"], text=True).strip()

resp = c.post(
    "/contracts/withdrawals",
    json={
        "name": "Kanarienvogel Nachname",
        "email": "kanarienvogel@example.invalid",
        "contract": "OTHER",
        "details": "Kanarienstrasse 42",
    },
)
assert resp.status_code == 200, resp.text

log = ""
for _ in range(20):
    log = subprocess.check_output(["journalctl", "-u", "academy-backend", "--since", since], text=True)
    if "Failed to send contract withdrawal confirmation email" in log:
        break
    time.sleep(0.5)
assert "Failed to send contract withdrawal confirmation email" in log, log
for secret in ["Kanarienvogel", "kanarienvogel@example.invalid", "Kanarienstrasse"]:
    assert secret not in log, f"{secret} reached the log:\n{log}"

assert os.system("systemctl start postfix.service") == 0


def declaration_count():
    status, out = subprocess.getstatusoutput(
        "sudo -u postgres psql -t --csv academy <<< 'select count(*) from contract_declarations'"
    )
    assert status == 0, out
    return int(out.strip())


# A declaration is kept as evidence until a claim out of the declared contract
# is time-barred: three years, counted from the end of the calendar year in
# which it was received (`contract.retention_years`).
recorded = declaration_count()
assert recorded > 0

assert os.system("systemctl start academy-task-prune-database.service") == 0
time.sleep(1)
assert declaration_count() == recorded

os.system("date -s '+4years'")
time.sleep(0.5)
assert os.system("systemctl start academy-task-prune-database.service") == 0
time.sleep(1)
assert declaration_count() == 0
