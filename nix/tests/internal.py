import subprocess

from utils import c

assert subprocess.getstatusoutput("academy migrate demo --force")[0] == 0


def prepare(aud: str):
    status, jwt = subprocess.getstatusoutput(f'academy jwt sign \'{{"aud":"{aud}"}}\'')
    assert status == 0
    jwt = jwt.strip()
    c.headers["Authorization"] = jwt


FOO = {
    "id": "a8d95e0f-71ae-4c49-995e-695b7c93848c",
    "name": "foo",
    "display_name": "Foo 42",
    "email": "foo@example.com",
    "email_verified": True,
    "registration": 1710423462,
    "last_login": 1710509820,
    "last_name_change": 1710424200,
    "enabled": True,
    "admin": False,
    "password": True,
    "mfa_enabled": False,
    "description": "blubb",
    "tags": ["foo", "bar", "baz"],
    "newsletter": True,
    "business": True,
    "first_name": "x",
    "last_name": "y",
    "street": "asdf",
    "zip_code": "1234",
    "city": "xyz",
    "country": "asdf",
    "vat_id": "1234",
    "can_buy_coins": True,
    "can_receive_coins": True,
    "avatar_url": "https://gravatar.com/avatar/321ba197033e81286fedb719d60d4ed5cecaed170733cb4a92013811afc0e3b6",
}

# auth
c.headers["Authorization"] = "blubb"
resp = c.get("/auth/_internal/users/a8d95e0f-71ae-4c49-995e-695b7c93848c")
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid token"}

prepare("auth")

## get user by id
resp = c.get("/auth/_internal/users/a8d95e0f-71ae-4c49-995e-695b7c93848c")
assert resp.status_code == 200
assert resp.json() == FOO

resp = c.get("/auth/_internal/users/85bae8d0-5419-48ba-9018-88df147a0eb2")
assert resp.status_code == 404
assert resp.json() == {"detail": "User not found"}

## get user by email
resp = c.get("/auth/_internal/users/by_email/Foo@example.com")
assert resp.status_code == 200
assert resp.json() == FOO

resp = c.get("/auth/_internal/users/by_email/not@found")
assert resp.status_code == 404
assert resp.json() == {"detail": "User not found"}

# shop
prepare("shop")

## add coins
resp = c.post(f"/shop/_internal/coins/{FOO['id']}", json={"coins": 1337, "description": "test", "credit_note": True})
assert resp.status_code == 200
assert resp.json() == {"coins": 1337, "withheld_coins": 0}

## remove coins
resp = c.post(f"/shop/_internal/coins/{FOO['id']}", json={"coins": -42, "description": "test2"})
assert resp.status_code == 200
assert resp.json() == {"coins": 1295, "withheld_coins": 0}

assert c.post(f"/shop/_internal/coins/{FOO['id']}", json={"coins": -1200, "description": "test3"}).json() == {
    "coins": 95,
    "withheld_coins": 0,
}

resp = c.post(f"/shop/_internal/coins/{FOO['id']}", json={"coins": -100, "description": "test4"})
assert resp.status_code == 412
assert resp.json() == {"detail": "Not enough coins"}

## add withhold
resp = c.post("/shop/_internal/coins/94d0e3ca-bf16-486b-a172-b87f4bcbd039", json={"coins": 42, "description": "test"})
assert resp.status_code == 200
assert resp.json() == {"coins": 0, "withheld_coins": 42}
