import os
from datetime import date

from utils import c, create_admin_account, create_verified_account, make_client, refresh_session, paypal_order
from utils import configure_purchases, make_internal_client, reserve_earned_coin_operation

configure_purchases()


a = create_verified_account("a", "a@a", "a")
c.patch("/auth/users/me", json={"business": False, "country": "Germany"})
order_id_a = paypal_order(1337, c)
c.post(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id_a}/confirm-payment-source")
c.post(f"/shop/coins/paypal/orders/{order_id_a}/capture")

b = create_verified_account("b", "b@b", "b")
# Recovery of an earned credit uses the existing invoice-information eligibility.
profile = c.patch(
    "/auth/users/me",
    json={
        "business": False,
        "country": "Germany",
        "first_name": "Test",
        "last_name": "Customer",
        "street": "Teststrasse 1",
        "zip_code": "12345",
        "city": "Teststadt",
    },
)
assert profile.status_code == 200
assert profile.json()["can_receive_coins"] is True
order_id_b = paypal_order(1337, c)
c.post(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id_b}/confirm-payment-source")
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
assert resp.status_code == 403
# A genuinely earned legacy claim still produces its credit note when recovered.
operation_id = reserve_earned_coin_operation(b["user"]["id"], 1337, "hello world")
internal = make_internal_client("shop")
resp = internal.put(
    f"/shop/_internal/coin-operations/{operation_id}/{b['user']['id']}",
    json={"coins": 1337, "description": "hello world", "credit_note": True},
)
assert resp.status_code == 200
assert resp.json() == {"coins": 2674, "withheld_coins": 0}

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
