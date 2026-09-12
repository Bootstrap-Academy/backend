import re
from io import BytesIO

from pypdf import PdfReader

from utils import (
    c,
    create_verified_account,
    decode_mail_header,
    decode_mail_part,
    fetch_mail,
    get_mail_parts,
    purchase_acceptance,
)
from utils import configure_purchases

configure_purchases()


login = create_verified_account("foobar", "foobar@example.com", "a")

assert c.get(f"/shop/coins/me").json() == {"coins": 0, "withheld_coins": 0}

# get client id
resp = c.get("/shop/coins/paypal")
assert resp.status_code == 200
assert resp.json() == "test-client"


# Missing typed acceptance cannot create an order.
resp = c.post("/shop/coins/paypal/orders", json={"coins": 1337})
assert resp.status_code == 422

# Offers require verified invoice information before acceptance is possible.
resp = c.post("/shop/coins/paypal/offers/1337")
assert resp.status_code == 412
assert c.get("/shop/coins/me").json() == {"coins": 0, "withheld_coins": 0}

resp = c.patch(
    "/auth/users/me", json={"business": False, "country": "Germany", "first_name": "Foo", "last_name": "Bar"}
)
assert resp.status_code == 200
assert resp.json()["can_buy_coins"] is True

offer_response = c.post("/shop/coins/paypal/offers/1337")
assert offer_response.status_code == 200, offer_response.text
offer = offer_response.json()
acceptance = purchase_acceptance(offer)
for field in ["accepted", "early_performance_requested"]:
    resp = c.post("/shop/coins/paypal/orders", json={"coins": 1337, **acceptance, field: False})
    assert resp.status_code == 409
    assert c.get("/shop/coins/me").json() == {"coins": 0, "withheld_coins": 0}

# Matching saved offer, explicit acceptance and exact replay use one provider order.
resp = c.post("/shop/coins/paypal/orders", json={"coins": 1337, **acceptance})
assert resp.status_code == 200, resp.text
order_id = resp.json()
assert c.post("/shop/coins/paypal/orders", json={"coins": 1337, **acceptance}).json() == order_id
pending = c.get(f"/shop/purchases/{offer['offer']['id']}").json()
assert pending["state"] == "awaiting_payment"
assert pending["confirmation_smtp_accepted_at"] is None
assert c.get("/shop/coins/me").json() == {"coins": 0, "withheld_coins": 0}

assert c.get(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}").json()["status"] == "CREATED"

# try to capture (not confirmed yet)
resp = c.post(f"/shop/coins/paypal/orders/{order_id}/capture")
assert resp.status_code == 503
assert "payment is still being checked" in resp.json()["detail"]
assert c.get(f"/shop/coins/me").json() == {"coins": 0, "withheld_coins": 0}
assert c.get(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}").json()["status"] == "CREATED"

# confirm order (client)
assert (
    c.post(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}/confirm-payment-source").json()["status"] == "APPROVED"
)

# capture order
resp = c.post(f"/shop/coins/paypal/orders/{order_id}/capture")
assert resp.status_code == 200
assert resp.json() == {"coins": 1337, "withheld_coins": 0}
assert c.get(f"/shop/coins/me").json() == {"coins": 1337, "withheld_coins": 0}
assert c.get(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}").json()["status"] == "COMPLETED"

# try to capture again
resp = c.post(f"/shop/coins/paypal/orders/{order_id}/capture")
assert resp.status_code == 200
assert resp.json() == {"coins": 1337, "withheld_coins": 0}
assert c.get(f"/shop/coins/me").json() == {"coins": 1337, "withheld_coins": 0}
assert c.get(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}").json()["status"] == "COMPLETED"

# Confirmation follows capture. Classify the two accepted SMTP messages by
# their attachments, without assuming Maildir directory order.
messages = [fetch_mail(), fetch_mail()]
confirmation = next(m for m in messages if len(get_mail_parts(m)) == 3)
mail = next(m for m in messages if len(get_mail_parts(m)) == 4)
assert decode_mail_header(confirmation["Subject"]) == "Deine Vertragsbestätigung – Bootstrap Academy"
confirmation_body, confirmation_terms, confirmation_withdrawal = get_mail_parts(confirmation)
assert offer["offer"]["id"] in decode_mail_part(confirmation_body).decode()
assert mail["X-Original-To"] == "foobar@example.com"
assert decode_mail_header(mail["Subject"]) == "Deine Vertragsbestätigung – Bootstrap Academy"
payload, invoice, terms, revocation_policy = get_mail_parts(mail)
content = decode_mail_part(payload).decode()
assert content == decode_mail_part(confirmation_body).decode()
assert offer["offer"]["text"] in content
assert offer["offer"]["declaration"] in content
assert "13.37 EUR" in content
assert invoice.get_filename() == "Rechnung-R0000001.pdf"
assert invoice["Content-Type"] == "application/pdf"
invoice_pdf = decode_mail_part(invoice)
pdf = PdfReader(BytesIO(invoice_pdf))
assert pdf.metadata and pdf.metadata.title == "Rechnung"
assert len(pdf.pages) == 1
invoice_text = pdf.pages[0].extract_text()
# every number on the invoice is formatted the German way; the net unit price
# keeps four decimal places, because it is the value that multiplies out to the
# net total of the line
assert "0,0084 €" in invoice_text
assert "1.337" in invoice_text
assert "Nettobetrag 11,24 €" in invoice_text
assert "zzgl. 19 % MwSt. 2,13 €" in invoice_text
assert "Gesamtbetrag 13,37 €" in invoice_text
assert "EUR" not in invoice_text
assert re.search(r"\bRechnungs-Nr\. *R0000001\b", invoice_text)
assert "Foo Bar" in invoice_text
assert "Germany" in invoice_text
assert "foobar@example.com" in invoice_text

# Both deliveries attach the exact stored documents selected by this order.
assert terms.get_filename() == "vereinbarte-agb.pdf"
assert terms["Content-Type"] == "application/pdf"
assert decode_mail_part(terms) == decode_mail_part(confirmation_terms)
assert revocation_policy.get_filename() == "vereinbarte-widerrufsinformation.pdf"
assert revocation_policy["Content-Type"] == "application/pdf"
assert decode_mail_part(revocation_policy) == decode_mail_part(confirmation_withdrawal)
for kind, attachment in [("terms", terms), ("withdrawal", revocation_policy)]:
    original = c.get(f"/shop/purchases/{offer['offer']['id']}/documents/{kind}")
    assert original.status_code == 200
    assert original.content == decode_mail_part(attachment)

assert open("/var/lib/academy/invoices/R0000001.pdf", "rb").read() == invoice_pdf
