import subprocess
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
    "cancellation_type",
    "details",
    "requested_end",
    "effective_end",
    "processed_at",
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

resp = c.post("/shop/premium", json={"plan": "MONTHLY", "autopay": True})
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
assert cancellation["cancellation_type"] == "ORDINARY"
assert cancellation["details"] == "Zu teuer"
assert cancellation["requested_end"] is None
assert cancellation["processed_at"] is None

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
assert "Die automatische Verlängerung ist abgeschaltet" in content
assert "Diese Bestätigung erfolgt nach § 312k Abs. 4 BGB." in content

notification = mails["contact@academy"]
assert decode_mail_header(notification["Subject"]) == "[Contract] Kündigung (Premium)"
content = decode_mail_payload(notification)
assert "Art der Erklärung: Kündigung" in content
assert f"Konto: {user_id}" in content
assert "Vertrag: Premium-Mitgliedschaft" in content
assert "Art der Kündigung: ordentliche Kündigung" in content
assert f"ID der Erklärung: {cancellation['id']}" in content

# declare a withdrawal
resp = c.post(
    "/contracts/withdrawals",
    json={"name": "Dieter Mustermann", "email": "dieter@example.com", "contract": "COINS", "details": None},
)
assert resp.status_code == 200
result = resp.json()
assert result["confirmation_email_sent"] is True

withdrawal = result["declaration"]
assert set(withdrawal) == DECLARATION_KEYS
assert withdrawal["kind"] == "WITHDRAWAL"
assert withdrawal["contract"] == "COINS"
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

# the admin listing exposes the matched account
ca = make_client()
create_admin_account("admin", "admin@example.com", "supersecureadminpassword", ca)

resp = ca.get("/contracts/declarations")
assert resp.status_code == 200
listing = resp.json()
assert listing["total"] == 2
assert [d["kind"] for d in listing["declarations"]] == ["WITHDRAWAL", "CANCELLATION"]
assert all(set(d) == DECLARATION_KEYS | {"user_id"} for d in listing["declarations"])
assert all(d["user_id"] == user_id for d in listing["declarations"])

resp = ca.get("/contracts/declarations", params={"kind": "CANCELLATION"})
assert resp.status_code == 200
listing = resp.json()
assert listing["total"] == 1
assert [d["id"] for d in listing["declarations"]] == [cancellation["id"]]

resp = ca.get("/contracts/declarations", params={"limit": 1, "offset": 1})
assert resp.status_code == 200
listing = resp.json()
assert listing["total"] == 2
assert [d["id"] for d in listing["declarations"]] == [cancellation["id"]]

# the admin listing requires admin privileges
resp = c.get("/contracts/declarations")
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

anonymous = make_client()
resp = anonymous.get("/contracts/declarations")
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid token"}

# the rate limit allows five declarations per hour and client ip address
for i in range(3):
    resp = c.post(
        "/contracts/withdrawals",
        json={"name": "Somebody Else", "email": f"somebody{i}@example.com", "contract": "OTHER", "details": None},
    )
    assert resp.status_code == 200, resp.text

resp = c.post(
    "/contracts/withdrawals",
    json={"name": "Somebody Else", "email": "somebody3@example.com", "contract": "OTHER", "details": None},
)
assert resp.status_code == 429
assert resp.json() == {"detail": "Too many requests"}

# nothing has been stored for the rejected declaration
resp = ca.get("/contracts/declarations")
assert resp.status_code == 200
assert resp.json()["total"] == 5
