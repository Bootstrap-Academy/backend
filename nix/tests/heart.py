import os
import subprocess

from utils import (
    c,
    create_verified_account,
    make_internal_client,
    refresh_session,
    purchase,
    purchase_offer,
    purchase_acceptance,
)
from utils import configure_purchases, seed_existing_coin_balance

configure_purchases()


login = create_verified_account("a", "a@a", "a")


# config
resp = c.get("/shop/hearts/config")
assert resp.status_code == 200
assert resp.json() == {"hearts_max": 6, "hearts_refill_price": 50}

# get
resp = c.get("/shop/hearts/me")
assert resp.status_code == 200
assert resp.json() == {"hearts": 6}

# get internal
ci = make_internal_client("shop")
resp = ci.get(f"/shop/_internal/hearts/{login['user']['id']}")
assert resp.status_code == 200
assert resp.json() == {"hearts": 6}

# remove hearts
resp = ci.post(f"/shop/_internal/hearts/{login['user']['id']}", json={"hearts": -2})
assert resp.status_code == 200
assert resp.json() is True

resp = c.get("/shop/hearts/me")
assert resp.status_code == 200
assert resp.json() == {"hearts": 4}

resp = ci.post(f"/shop/_internal/hearts/{login['user']['id']}", json={"hearts": -5})
assert resp.status_code == 200
assert resp.json() is False

resp = c.get("/shop/hearts/me")
assert resp.status_code == 200
assert resp.json() == {"hearts": 4}

# add hearts
resp = ci.post(f"/shop/_internal/hearts/{login['user']['id']}", json={"hearts": 7})
assert resp.status_code == 200
assert resp.json() is True

resp = c.get("/shop/hearts/me")
assert resp.status_code == 200
assert resp.json() == {"hearts": 6}

# auto refill
resp = ci.post(f"/shop/_internal/hearts/{login['user']['id']}", json={"hearts": -5})
assert resp.status_code == 200
assert resp.json() is True

resp = c.get("/shop/hearts/me")
assert resp.status_code == 200
assert resp.json() == {"hearts": 1}

os.system("date -s '+18hours'")

refresh_session()

resp = c.get("/shop/hearts/me")
assert resp.status_code == 200
assert resp.json() == {"hearts": 6}

# manual refill
resp = ci.post(f"/shop/_internal/hearts/{login['user']['id']}", json={"hearts": -4})
assert resp.status_code == 200
assert resp.json() is True

resp = c.get("/shop/hearts/me")
assert resp.status_code == 200
assert resp.json() == {"hearts": 2}

offer = purchase_offer("hearts")
for field in ["accepted", "early_performance_requested"]:
    resp = c.post("/shop/purchases/accept", json={**purchase_acceptance(offer), field: False})
    assert resp.status_code == 409
    assert c.get("/shop/coins/me").json()["coins"] == 0

# Insufficient funds preserve the balance/hearts and the rejected original.
resp = c.post("/shop/purchases/accept", json=purchase_acceptance(offer))
assert resp.status_code == 200, resp.text
assert resp.json()["state"] == "failed"
assert resp.json()["review_reason"] == "Not enough coins; order rejected without charge"
assert resp.json()["confirmation_smtp_accepted_at"] is None

resp = c.get("/shop/hearts/me")
assert resp.status_code == 200
assert resp.json() == {"hearts": 2}

## ok
seed_existing_coin_balance(login["user"]["id"], 70)
assert c.get(f"/shop/coins/me").json()["coins"] == 70

purchase("hearts")

resp = c.get("/shop/hearts/me")
assert resp.status_code == 200
assert resp.json() == {"hearts": 6}

assert c.get(f"/shop/coins/me").json()["coins"] == 20
