import calendar
import os
import subprocess
import time
from datetime import datetime, timezone

from utils import (
    c,
    create_verified_account,
    make_internal_client,
    save_auth,
    enable_premium_renewal,
    purchase,
    purchase_offer,
    purchase_acceptance,
)
from utils import configure_purchases, seed_existing_coin_balance

configure_purchases()


def assert_one_month(status):
    """Assert that the period runs for one calendar month.

    A month is the same day of the next month, or its last day if that day does
    not exist there (§ 188 Abs. 2 and Abs. 3 BGB), and not a fixed number of
    days. The backend counts the day in `Europe/Berlin`; the hour of the day
    used here is far enough from midnight for that to make no difference, and
    the tolerance absorbs the summer time offset.
    """
    since = datetime.fromtimestamp(status["since"], timezone.utc)
    year, month = (since.year, since.month + 1) if since.month < 12 else (since.year + 1, 1)
    day = min(since.day, calendar.monthrange(year, month)[1])
    expected = since.replace(year=year, month=month, day=day).timestamp()
    assert abs(status["until"] - expected) <= 3601, (status, expected)


login = create_verified_account("a", "a@a", "a")
ci = make_internal_client("shop")

# list plans
resp = c.get("/shop/premium_plans")
assert resp.status_code == 200
assert resp.json() == {"MONTHLY": {"price": 1000, "months": 1}, "YEARLY": {"price": 10000, "months": 12}}

# get status (no premium yet)
resp = c.get("/shop/premium/me")
assert resp.status_code == 200
assert resp.json() == {"premium": False, "since": None, "until": None, "autopay": None, "renewal": None}

# get internal
resp = ci.get(f"/shop/_internal/premium/{login['user']['id']}")
assert resp.status_code == 200
assert resp.json() is False

# Explicit acceptance is required before any coin debit.
offer = purchase_offer("premium_monthly")
for field in ["accepted", "early_performance_requested"]:
    resp = c.post("/shop/purchases/accept", json={**purchase_acceptance(offer), field: False})
    assert resp.status_code == 409
    assert c.get("/shop/coins/me").json()["coins"] == 0

# Insufficient funds are a terminal, saved rejection, not a successful purchase.
resp = c.post("/shop/purchases/accept", json=purchase_acceptance(offer))
assert resp.status_code == 200, resp.text
assert resp.json()["state"] == "failed"
assert resp.json()["review_reason"] == "Not enough coins; order rejected without charge"
assert resp.json()["confirmation_smtp_accepted_at"] is None
assert c.get("/shop/premium/me").json()["premium"] is False

## ok
seed_existing_coin_balance(login["user"]["id"], 15000)
start = time.time() - 1
purchase("premium_monthly")
end = time.time() + 1
status = c.get("/shop/premium/me").json()
assert status["premium"] is True
assert status["autopay"] is None
assert start <= status["since"] <= end
assert_one_month(status)
assert c.get("/shop/premium/me").json() == status
assert c.get("/shop/coins/me").json()["coins"] == 14000

# get internal
resp = ci.get(f"/shop/_internal/premium/{login['user']['id']}")
assert resp.status_code == 200
assert resp.json() is True
assert c.get("/shop/premium/me").json()["autopay"] is None

# premium expires
os.system(f"date -s @{int(status["until"] + 2)}")
save_auth(login := c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"}).json())

assert c.get("/shop/premium/me").json() == {
    "premium": False,
    "since": None,
    "until": None,
    "autopay": None,
    "renewal": None,
}

# purchase with subscription
start = time.time() - 1
purchase("premium_monthly")
end = time.time() + 1
status = enable_premium_renewal(c)
assert status["premium"] is True
assert status["autopay"] == "MONTHLY"
assert start <= status["since"] <= end
assert_one_month(status)
assert c.get("/shop/premium/me").json() == status
assert c.get("/shop/coins/me").json()["coins"] == 13000

os.system("date -s '+32days'")
save_auth(login := c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"}).json())
status = c.get("/shop/premium/me").json()
assert status["premium"] is True
assert status["autopay"] == "MONTHLY"
assert c.get("/shop/coins/me").json()["coins"] == 12000

# update subscription
resp = c.put("/shop/premium/autopay", json={"plan": "YEARLY"})
assert resp.status_code == 412
assert c.get("/shop/premium/me").json()["autopay"] == "MONTHLY"

# Rejected yearly activation leaves the explicitly agreed monthly renewal intact.
os.system("date -s '+32days'")
save_auth(login := c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"}).json())
status = c.get("/shop/premium/me").json()
assert status["premium"] is True
assert status["autopay"] == "MONTHLY"
assert c.get("/shop/coins/me").json()["coins"] == 11000
assert_one_month(status)

assert c.put("/shop/premium/autopay", json={"plan": "MONTHLY"}).status_code == 412

os.system("date -s '+367days'")
save_auth(login := c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"}).json())
os.system("systemctl start --wait academy-task-refresh-premium.service")
assert c.get("/shop/coins/me").json()["coins"] == 10000
status = c.get("/shop/premium/me").json()
assert status["premium"] is True
assert status["autopay"] == "MONTHLY"

# leave just enough coins for a single renewal
assert subprocess.getstatusoutput(f"academy admin coin add {login['user']['id']} -- -9000")[0] == 0
assert c.get("/shop/coins/me").json()["coins"] == 1000

os.system("date -s '+32days'")
save_auth(login := c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"}).json())
status = c.get("/shop/premium/me").json()
assert status["premium"] is True
assert status["autopay"] == "MONTHLY"
assert c.get("/shop/coins/me").json()["coins"] == 0

os.system("date -s '+32days'")
save_auth(login := c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"}).json())
status = c.get("/shop/premium/me").json()
assert status["premium"] is False
assert status["autopay"] is None
assert c.get("/shop/coins/me").json()["coins"] == 0

resp = ci.get(f"/shop/_internal/premium/{login['user']['id']}")
assert resp.status_code == 200
assert resp.json() is False
