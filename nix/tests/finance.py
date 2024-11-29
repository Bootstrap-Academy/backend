import os
from datetime import date

from utils import c, create_admin_account, create_verified_account, make_client, refresh_session

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

# invoices
resp = c.get(f"/finance/invoices/{token}/2/invoice.pdf")
assert resp.status_code == 200
assert resp.content == open("/var/lib/academy/invoices/R0000002.pdf", "rb").read()

resp = c.get(f"/finance/invoices/{token}/1/invoice.pdf")
assert resp.status_code == 404
assert resp.json() == {"detail": "Invoice not found"}

# credit notes
c2 = make_client()
create_admin_account("adm", "adm@example.com", "adm", c2)
resp = c2.post(f"/shop/coins/{b['user']['id']}", json={"coins": 1337, "description": "hello world"})
assert resp.status_code == 200

today = date.today()
resp = c.get(f"/finance/credit_notes/{token}/{today.year}/{today.month}/credit_note.pdf")
assert resp.status_code == 404
assert resp.json() == {"detail": "Credit note not yet available"}

os.system("date -s '+20days'")
refresh_session()
os.system("date -s '+20days'")
refresh_session()

resp = c.get(f"/finance/credit_notes/{token}/{today.year}/{today.month}/credit_note.pdf")
assert resp.status_code == 401
assert resp.json() == {"detail": "Invalid token"}

resp = c.get("/finance/token")
assert resp.status_code == 200
token = resp.json()

resp = c.get(f"/finance/credit_notes/{token}/{today.year}/{today.month}/credit_note.pdf")
assert resp.status_code == 200
assert resp.content == open(f"/var/lib/academy/credit_notes/G{today.year:04}{today.month:02}-1.pdf", "rb").read()
