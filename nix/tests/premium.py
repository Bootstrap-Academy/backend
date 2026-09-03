import os
import subprocess
import time

from utils import c, create_verified_account, make_internal_client, save_auth

login = create_verified_account("a", "a@a", "a")
ci = make_internal_client("shop")

# list plans
resp = c.get("/shop/premium_plans")
assert resp.status_code == 200
assert resp.json() == {"MONTHLY": {"price": 1000, "months": 1}, "YEARLY": {"price": 10000, "months": 12}}

# get status (no premium yet)
resp = c.get("/shop/premium/me")
assert resp.status_code == 200
assert resp.json() == {"premium": False, "since": None, "until": None, "autopay": None}

# get internal
resp = ci.get(f"/shop/_internal/premium/{login['user']['id']}")
assert resp.status_code == 200
assert resp.json() is False

# purchase
## not enough coins
resp = c.post("/shop/premium", json={"plan": "MONTHLY"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Not enough coins"}
assert c.get("/shop/premium/me").json()["premium"] is False

## ok
assert subprocess.getstatusoutput(f"academy admin coin add {login['user']['id']} 15000")[0] == 0
start = time.time() - 1
resp = c.post("/shop/premium", json={"plan": "MONTHLY"})
end = time.time() + 1
assert resp.status_code == 200
status = resp.json()
assert status["premium"] is True
assert status["autopay"] is None
assert start <= status["since"] <= end
assert abs(status["until"] - status["since"] - 3600 * 24 * 365.25 / 12) <= 2
assert c.get("/shop/premium/me").json() == status
assert c.get("/shop/coins/me").json()["coins"] == 14000

# get internal
resp = ci.get(f"/shop/_internal/premium/{login['user']['id']}")
assert resp.status_code == 200
assert resp.json() is True

# premium expires
os.system(f"date -s @{int(status["until"] + 2)}")
save_auth(login := c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"}).json())

assert c.get("/shop/premium/me").json() == {"premium": False, "since": None, "until": None, "autopay": None}

# purchase with subscription
start = time.time() - 1
resp = c.post("/shop/premium", json={"plan": "MONTHLY", "autopay": True})
end = time.time() + 1
assert resp.status_code == 200
status = resp.json()
assert status["premium"] is True
assert status["autopay"] == "MONTHLY"
assert start <= status["since"] <= end
assert abs(status["until"] - status["since"] - 3600 * 24 * 365.25 / 12) <= 2
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
assert resp.status_code == 200
assert resp.json() is True

# a yearly subscription is renewed month by month at the monthly price and the
# stored subscription is updated accordingly
os.system("date -s '+32days'")
save_auth(login := c.post("/auth/sessions", json={"name_or_email": "a", "password": "a"}).json())
status = c.get("/shop/premium/me").json()
assert status["premium"] is True
assert status["autopay"] == "MONTHLY"
assert c.get("/shop/coins/me").json()["coins"] == 11000
assert abs(status["until"] - status["since"] - 3600 * 24 * 365.25 / 12) <= 2

assert c.put("/shop/premium/autopay", json={"plan": "MONTHLY"}).status_code == 200

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
