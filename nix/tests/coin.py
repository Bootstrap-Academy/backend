from utils import c, create_account

login = create_account("a", "a@a", "a")

resp = c.get(f"/shop/coins/me")
assert resp.status_code == 200
assert resp.json() == {"coins": 0, "withheld_coins": 0}
