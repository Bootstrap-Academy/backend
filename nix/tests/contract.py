"""Public receipt privacy, contract-specific effects and durable operational evidence."""

import os
import subprocess
import time
import uuid

from utils import (
    c,
    create_admin_account,
    create_verified_account,
    enable_premium_renewal,
    fetch_mail,
    make_client,
    purchase,
)
from utils import configure_purchases, seed_existing_coin_balance

configure_purchases()


PUBLIC_KEYS = {
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
login = create_verified_account("dieter", "dieter@example.com", "supersecurepassword")
user_id = login["user"]["id"]
seed_existing_coin_balance(user_id, 5000)
purchase("premium_monthly", c)
fetch_mail()  # confirmation of the separate one-off purchase
premium = enable_premium_renewal(c)
fetch_mail()  # separate T3 agreement confirmation
anonymous = make_client()
body = {
    "name": "Dieter Mustermann",
    "email": "dieter@example.com",
    "contract": "PREMIUM",
    "contract_designation": "Premium monatlich",
    "cancellation_type": "ORDINARY",
    "details": "Original reasoning",
    "requested_end": None,
    "request_key": {"id": str(uuid.uuid4()), "secret": str(uuid.uuid4())},
}
response = anonymous.post("/contracts/cancellations", json=body)
assert response.status_code == 200, response.text
receipt = response.json()
assert set(receipt["declaration"]) == PUBLIC_KEYS
assert receipt["declaration"]["id"] == body["request_key"]["id"]
assert c.get("/shop/premium/me").json() == premium  # email alone cannot mutate another account
assert anonymous.post("/contracts/cancellations", json=body).json()["declaration"] == receipt["declaration"]
assert anonymous.post("/contracts/receipts", json=body["request_key"]).json()["declaration"] == receipt["declaration"]
assert (
    anonymous.post("/contracts/receipts", json={**body["request_key"], "secret": str(uuid.uuid4())}).status_code == 404
)
assert anonymous.post("/contracts/cancellations", json={**body, "details": "changed"}).status_code == 409

identified = {
    **body,
    "renewal_agreement_id": premium["renewal"]["id"],
    "request_key": {"id": str(uuid.uuid4()), "secret": str(uuid.uuid4())},
}
response = anonymous.post("/contracts/cancellations", json=identified)
assert response.status_code == 200, response.text
assert set(response.json()["declaration"]) == PUBLIC_KEYS
assert c.get("/shop/premium/me").json() == {**premium, "autopay": None, "renewal": None}
extraordinary = anonymous.post(
    "/contracts/cancellations",
    json={
        **body,
        "request_key": {"id": str(uuid.uuid4()), "secret": str(uuid.uuid4())},
        "cancellation_type": "EXTRAORDINARY",
    },
).json()["declaration"]
withdrawal = anonymous.post(
    "/contracts/withdrawals",
    json={"name": "Dieter", "email": "dieter@example.com", "contract": "COINS", "contract_designation": "Order 4711"},
)
assert withdrawal.status_code == 200
assert set(withdrawal.json()["declaration"]) == PUBLIC_KEYS
subprocess.run(["academy", "task", "retry-contract-confirmations"], check=True)

admin = make_client()
create_admin_account("admin", "admin@example.com", "supersecureadminpassword", admin)
listing = admin.get("/contracts/declarations").json()
assert listing["total"] == 4
assert all(d["user_id"] == user_id for d in listing["declarations"])
assert all(d["delivery"] and d["operational_evidence"] for d in listing["declarations"])
assert any(d["effective_end"] for d in listing["declarations"])
endpoint = f"/contracts/declarations/{extraordinary['id']}"
resolution = {
    "action": "RECORD_EXTERNAL_RESOLUTION",
    "identity_verified": True,
    "effective_end": "2026-10-31T22:59:59Z",
    "note": "Identity independently verified, original receipt rights assessed; external action and communication recorded",
}
assert admin.patch(endpoint, json={**resolution, "identity_verified": False}).status_code == 400
processed = admin.patch(endpoint, json=resolution)
assert processed.status_code == 200, processed.text
assert processed.json()["effective_end"].startswith("2026-10-31T22:59:59")
assert processed.json()["processing_note"] == resolution["note"]
assert admin.patch(endpoint, json=resolution).status_code == 409
assert c.patch(endpoint, json=resolution).status_code == 403
assert c.get("/contracts/declarations").status_code == 403
assert anonymous.get("/contracts/declarations").status_code == 401
assert anonymous.post("/contracts/receipts", json=body["request_key"]).json()["declaration"] == receipt["declaration"]

# SMTP failure does not undo the legally received declaration; no personal fields in logs.
assert os.system("systemctl stop postfix.service") == 0
since = subprocess.check_output(["date", "+%Y-%m-%d %H:%M:%S"], text=True).strip()
response = anonymous.post(
    "/contracts/withdrawals",
    json={
        "name": "Kanarienvogel Nachname",
        "email": "kanarienvogel@example.invalid",
        "contract": "OTHER",
        "details": "Kanarienstrasse 42",
    },
)
assert response.status_code == 200, response.text
assert response.json()["confirmation_email_sent"] is False
for _ in range(20):
    log = subprocess.check_output(["journalctl", "-u", "academy-backend", "--since", since], text=True)
    if "Declaration delivery failed; retry scheduled" in log:
        break
    time.sleep(0.5)
assert "Declaration delivery failed; retry scheduled" in log, log
assert all(secret not in log for secret in ["Kanarienvogel", "kanarienvogel@example.invalid", "Kanarienstrasse"])
assert os.system("systemctl start postfix.service") == 0


# Open declarations and pending delivery evidence survive ordinary historical pruning.
def count():
    output = subprocess.check_output(
        ["sudo", "-u", "postgres", "psql", "-d", "academy", "-tAc", "SELECT count(*) FROM contract_declarations"],
        text=True,
    )
    return int(output.strip())


recorded = count()
subprocess.run(["systemctl", "start", "--wait", "academy-task-prune-database.service"], check=True)
assert count() == recorded
