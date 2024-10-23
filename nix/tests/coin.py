from utils import c, create_account, create_admin_account, make_client

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
assert resp.status_code == 200
assert resp.json() == True

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
