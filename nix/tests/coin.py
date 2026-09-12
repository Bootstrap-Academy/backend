import subprocess

from utils import c, create_account, create_admin_account, make_client, seed_existing_coin_balance

# config (public)
resp = c.get("/shop/coins/config")
assert resp.status_code == 200
assert resp.json() == {"coins_per_euro": 100, "vat_percent": 19.0}

login = create_account("a", "a@a", "a")

# get balance
resp = c.get(f"/shop/coins/me")
assert resp.status_code == 200
assert resp.json() == {"coins": 0, "withheld_coins": 0}

# add coins
resp = c.post(f"/shop/coins/me", json={"coins": 1337, "description": "test", "credit_note": True})
assert resp.status_code == 403
assert resp.json() == {"detail": "Permission denied"}

resp = c.get(f"/shop/coins/me")
assert resp.status_code == 200
assert resp.json() == {"coins": 0, "withheld_coins": 0}

adm = make_client()
adm_login = create_admin_account("admin", "admin@admin", "admin", adm)

resp = adm.post(f"/shop/coins/{login['user']['id']}", json={"coins": 1337, "description": "test", "credit_note": True})
assert resp.status_code == 403
assert resp.json() == {"detail": "Use the purchase or recovery path to credit coins"}
assert c.get("/shop/coins/me").json() == {"coins": 0, "withheld_coins": 0}

# The CLI cannot turn an administrator into a new source of free coins either.
result = subprocess.run(
    ["academy", "admin", "coin", "add", login["user"]["id"], "1337"], capture_output=True, text=True
)
assert result.returncode != 0
assert "Use the purchase or verified recovery path" in result.stderr
assert c.get("/shop/coins/me").json() == {"coins": 0, "withheld_coins": 0}

# Existing balances remain usable for legitimate nonpositive adjustments.
seed_existing_coin_balance(login["user"]["id"], 1337)
resp = c.get(f"/shop/coins/me")
assert resp.status_code == 200
assert resp.json() == {"coins": 1337, "withheld_coins": 0}

## remove coins
resp = adm.post(f"/shop/coins/{login['user']['id']}", json={"coins": -42, "description": "asdf", "credit_note": False})
assert resp.status_code == 200
assert resp.json() == True

resp = c.get(f"/shop/coins/me")
assert resp.status_code == 200
assert resp.json() == {"coins": 1337 - 42, "withheld_coins": 0}

### not enough coins
resp = adm.post(f"/shop/coins/{login['user']['id']}", json={"coins": -1337, "description": "asdf"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Not enough coins"}

resp = c.get(f"/shop/coins/me")
assert resp.status_code == 200
assert resp.json() == {"coins": 1337 - 42, "withheld_coins": 0}

# withdrawal declarations for purchases that are completed by another service
## declarations missing
resp = c.post("/shop/consents", json={"subject": "course", "reference": "html"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Withdrawal consent missing"}

resp = c.post(
    "/shop/consents",
    json={"subject": "course", "reference": "html", "withdrawal_consent": False, "withdrawal_text_version": "2026-09"},
)
assert resp.status_code == 412
assert resp.json() == {"detail": "Withdrawal consent missing"}

## recorded
resp = c.post(
    "/shop/consents",
    json={"subject": "course", "reference": "html", "withdrawal_consent": True, "withdrawal_text_version": "2026-09"},
)
assert resp.status_code == 200
consent = resp.json()
assert consent["subject"] == "course"
assert consent["reference"] == "html"
assert consent["text_version"] == "2026-09"
assert consent["consented_at"] > 0

## a booking has no reference of its own
resp = c.post(
    "/shop/consents", json={"subject": "webinar", "withdrawal_consent": True, "withdrawal_text_version": "2026-09"}
)
assert resp.status_code == 200
assert resp.json()["reference"] is None

## unauthenticated
resp = make_client().post(
    "/shop/consents", json={"subject": "course", "withdrawal_consent": True, "withdrawal_text_version": "2026-09"}
)
assert resp.status_code == 401
