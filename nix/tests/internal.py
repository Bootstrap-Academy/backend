import subprocess

from utils import c, make_internal_client, reserve_earned_coin_operation, vm_sql

assert subprocess.getstatusoutput("academy migrate demo --force")[0] == 0


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
    "leaderboard_opt_out": False,
    "terms_version": "2024-03",
    "terms_accepted_at": 1710423462,
    "terms_declined_at": None,
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
    "avatar_url": None,
}

# auth
c.headers["Authorization"] = "blubb"
resp = c.get("/auth/_internal/users/a8d95e0f-71ae-4c49-995e-695b7c93848c")
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid token"}

c = make_internal_client("auth")

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
c = make_internal_client("shop")

## add coins
resp = c.post(f"/shop/_internal/coins/{FOO['id']}", json={"coins": 1337, "description": "test", "credit_note": True})
assert resp.status_code == 403
assert resp.json() == {"detail": "Use the purchase or recovery path to credit coins"}

# A new arbitrary operation ID must not create its own positive pending authority.
from uuid import uuid4

unknown = str(uuid4())
payload = {"coins": 1337, "description": "test", "credit_note": True}
assert c.put(f"/shop/_internal/coin-operations/{unknown}/{FOO['id']}", json=payload).status_code == 403
assert vm_sql(f"SELECT count(*) FROM internal_coin_operations WHERE id='{unknown}';") == "0"

# Exact prepared historical authority remains payable once and replayable.
operation_id = reserve_earned_coin_operation(FOO["id"], 1337, "test")
endpoint = f"/shop/_internal/coin-operations/{operation_id}/{FOO['id']}"
resp = c.put(endpoint, json=payload)
assert resp.status_code == 200
assert resp.json() == {"coins": 1337, "withheld_coins": 0}
assert c.put(endpoint, json=payload).json() == resp.json()
assert c.put(endpoint, json={**payload, "coins": 1338}).status_code == 409

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
assert resp.status_code == 403
withheld_user = "94d0e3ca-bf16-486b-a172-b87f4bcbd039"
operation_id = reserve_earned_coin_operation(withheld_user, 42, "test")
endpoint = f"/shop/_internal/coin-operations/{operation_id}/{withheld_user}"
resp = c.put(endpoint, json={"coins": 42, "description": "test", "credit_note": True})
assert resp.status_code == 200
assert resp.json() == {"coins": 0, "withheld_coins": 42}
assert c.put(endpoint, json={"coins": 42, "description": "test", "credit_note": True}).json() == resp.json()
