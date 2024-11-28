from utils import c, create_verified_account

a = create_verified_account("a", "a@a", "a")
c.patch("/auth/users/me", json={"business": False, "country": "Germany"})
order_id_a = c.post("/shop/coins/paypal/orders", json={"coins": 1337}).json()
c.post(f"http://127.0.0.1:8004/v2/checkout/orders/{order_id_a}/confirm-payment-source")
c.post(f"/shop/coins/paypal/orders/{order_id_a}/capture")

b = create_verified_account("b", "b@b", "b")
c.patch("/auth/users/me", json={"business": False, "country": "Germany"})
order_id_b = c.post("/shop/coins/paypal/orders", json={"coins": 1337}).json()
c.post(f"http://127.0.0.1:8004/v2/checkout/orders/{order_id_b}/confirm-payment-source")
c.post(f"/shop/coins/paypal/orders/{order_id_b}/capture")

resp = c.get("/finance/token")
assert resp.status_code == 200
token = resp.json()

resp = c.get(f"/finance/invoices/{token}/2/invoice.pdf")
assert resp.status_code == 200
assert resp.content == open("/var/lib/academy/invoices/R0000002.pdf", "rb").read()

resp = c.get(f"/finance/invoices/{token}/1/invoice.pdf")
assert resp.status_code == 404
assert resp.json() == {"detail": "Invoice not found"}
