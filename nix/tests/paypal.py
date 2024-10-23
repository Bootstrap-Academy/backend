from utils import c, create_verified_account

login = create_verified_account("a", "a@a", "a")

assert c.get(f"/shop/coins/me").json() == {"coins": 0, "withheld_coins": 0}

# get client id
resp = c.get("/shop/coins/paypal")
assert resp.status_code == 200
assert resp.json() == "test-client"


# create order
## invoice info missing
resp = c.post("/shop/coins/paypal/orders", json={"coins": 1337})
assert resp.status_code == 412
assert resp.json() == {"detail": "User Infos missing"}

resp = c.patch("/auth/users/me", json={"business": False, "country": "Germany"})
assert resp.status_code == 200
assert resp.json()["can_buy_coins"] is True

## success
resp = c.post("/shop/coins/paypal/orders", json={"coins": 1337})
print(resp.status_code, resp.json())
assert resp.status_code == 200
order_id = resp.json()

assert c.get(f"http://127.0.0.1:8004/v2/checkout/orders/{order_id}").json() == {"status": "Created", "coins": 1337}

# try to capture (not confirmed yet)
resp = c.post(f"/shop/coins/paypal/orders/{order_id}/capture")
assert resp.status_code == 400
assert resp.json() == {"detail": "Could not capture order"}
assert c.get(f"/shop/coins/me").json() == {"coins": 0, "withheld_coins": 0}
assert c.get(f"http://127.0.0.1:8004/v2/checkout/orders/{order_id}").json() == {"status": "Created", "coins": 1337}

# confirm order (client)
assert c.post(f"http://127.0.0.1:8004/v2/checkout/orders/{order_id}/confirm-payment-source").json() == {
    "status": "Confirmed",
    "coins": 1337,
}

# capture order
resp = c.post(f"/shop/coins/paypal/orders/{order_id}/capture")
assert resp.status_code == 200
assert resp.json() == {"coins": 1337, "withheld_coins": 0}
assert c.get(f"/shop/coins/me").json() == {"coins": 1337, "withheld_coins": 0}
assert c.get(f"http://127.0.0.1:8004/v2/checkout/orders/{order_id}").json() == {"status": "Captured"}

# try to capture again
resp = c.post(f"/shop/coins/paypal/orders/{order_id}/capture")
assert resp.status_code == 404
assert resp.json() == {"detail": "Order not found"}
assert c.get(f"/shop/coins/me").json() == {"coins": 1337, "withheld_coins": 0}
assert c.get(f"http://127.0.0.1:8004/v2/checkout/orders/{order_id}").json() == {"status": "Captured"}
