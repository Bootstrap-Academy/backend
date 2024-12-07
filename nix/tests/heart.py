import os
import subprocess

from utils import c, create_verified_account, make_internal_client, refresh_session

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

## not enough coins
assert c.get(f"/shop/coins/me").json()["coins"] == 0

resp = c.put("/shop/hearts")
assert resp.status_code == 412
assert resp.json() == {"detail": "Not enough coins"}

resp = c.get("/shop/hearts/me")
assert resp.status_code == 200
assert resp.json() == {"hearts": 2}

## ok
assert subprocess.getstatusoutput(f"academy admin coin add {login['user']['id']} 70")[0] == 0
assert c.get(f"/shop/coins/me").json()["coins"] == 70

resp = c.put("/shop/hearts")
assert resp.status_code == 200
assert resp.json() == {"hearts": 6}

resp = c.get("/shop/hearts/me")
assert resp.status_code == 200
assert resp.json() == {"hearts": 6}

assert c.get(f"/shop/coins/me").json()["coins"] == 20
