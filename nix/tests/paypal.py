import hashlib
import re
from io import BytesIO

from pypdf import PdfReader

from utils import c, create_verified_account, decode_mail_header, decode_mail_part, fetch_mail, get_mail_parts

login = create_verified_account("foobar", "foobar@example.com", "a")

assert c.get(f"/shop/coins/me").json() == {"coins": 0, "withheld_coins": 0}

# get client id
resp = c.get("/shop/coins/paypal")
assert resp.status_code == 200
assert resp.json() == "test-client"


# create order
## withdrawal declarations missing
resp = c.post("/shop/coins/paypal/orders", json={"coins": 1337})
assert resp.status_code == 412
assert resp.json() == {"detail": "Withdrawal consent missing"}

resp = c.post(
    "/shop/coins/paypal/orders", json={"coins": 1337, "withdrawal_consent": False, "withdrawal_text_version": "2026-09"}
)
assert resp.status_code == 412
assert resp.json() == {"detail": "Withdrawal consent missing"}

## invoice info missing
resp = c.post(
    "/shop/coins/paypal/orders", json={"coins": 1337, "withdrawal_consent": True, "withdrawal_text_version": "2026-09"}
)
assert resp.status_code == 412
assert resp.json() == {"detail": "User Infos missing"}

resp = c.patch(
    "/auth/users/me", json={"business": False, "country": "Germany", "first_name": "Foo", "last_name": "Bar"}
)
assert resp.status_code == 200
assert resp.json()["can_buy_coins"] is True

## success
resp = c.post(
    "/shop/coins/paypal/orders", json={"coins": 1337, "withdrawal_consent": True, "withdrawal_text_version": "2026-09"}
)
assert resp.status_code == 200
order_id = resp.json()

assert c.get(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}").json() == {"status": "Created", "coins": 1337}

# try to capture (not confirmed yet)
resp = c.post(f"/shop/coins/paypal/orders/{order_id}/capture")
assert resp.status_code == 400
assert resp.json() == {"detail": "Could not capture order"}
assert c.get(f"/shop/coins/me").json() == {"coins": 0, "withheld_coins": 0}
assert c.get(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}").json() == {"status": "Created", "coins": 1337}

# confirm order (client)
assert c.post(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}/confirm-payment-source").json() == {
    "status": "Confirmed",
    "coins": 1337,
}

# capture order
resp = c.post(f"/shop/coins/paypal/orders/{order_id}/capture")
assert resp.status_code == 200
assert resp.json() == {"coins": 1337, "withheld_coins": 0}
assert c.get(f"/shop/coins/me").json() == {"coins": 1337, "withheld_coins": 0}
assert c.get(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}").json() == {"status": "Captured"}

# try to capture again
resp = c.post(f"/shop/coins/paypal/orders/{order_id}/capture")
assert resp.status_code == 404
assert resp.json() == {"detail": "Order not found"}
assert c.get(f"/shop/coins/me").json() == {"coins": 1337, "withheld_coins": 0}
assert c.get(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}").json() == {"status": "Captured"}

# invoice email
mail = fetch_mail()
assert mail["X-Original-To"] == "foobar@example.com"
assert decode_mail_header(mail["Subject"]) == "Kaufbestätigung - Bootstrap Academy"
payload, invoice, terms, revocation_policy = get_mail_parts(mail)
content = decode_mail_part(payload).decode()
assert "Du hast erfolgreich 1337 MorphCoins gekauft! Das entspricht 13.37€ inklusive 19% MwSt. von 2.13€." in content
assert "Deine Erklärungen zum Widerrufsrecht bei dieser Bestellung" in content
assert (
    "Ich stimme ausdrücklich zu, dass Sie vor Ablauf der Widerrufsfrist mit der Ausführung des "
    "Vertrags beginnen. Mir ist bekannt, dass mein Widerrufsrecht mit Beginn der Ausführung des "
    "Vertrags erlischt." in content
)
assert "Fassung der Widerrufsbelehrung 2026-09" in content
assert "https://bootstrap.academy/docs/right-of-withdrawal" in content

assert invoice["Content-Disposition"] == 'attachment; filename="rechnung.pdf"'
assert invoice["Content-Type"] == "application/pdf"
invoice_pdf = decode_mail_part(invoice)
pdf = PdfReader(BytesIO(invoice_pdf))
assert pdf.metadata and pdf.metadata.title == "Rechnung"
assert len(pdf.pages) == 1
invoice_text = pdf.pages[0].extract_text()
assert "Nettobetrag 11.24 EUR" in invoice_text
assert "zzgl. 19% MwSt. 2.13 EUR" in invoice_text
assert "Gesamtbetrag 13.37 EUR" in invoice_text
assert re.search(r"\bRechnungs-Nr\. *R0000001\b", invoice_text)
assert "Foo Bar" in invoice_text
assert "Germany" in invoice_text
assert "foobar@example.com" in invoice_text

assert terms["Content-Disposition"] == 'attachment;\n filename*0="allgemeine_geschaeftsbedingungen.pdf"'
assert terms["Content-Type"] == "application/pdf"
hash = hashlib.sha256(decode_mail_part(terms)).hexdigest()
assert hash == "7a8568f6ee99b914463265a8e42bb9736f719aca832cee63019ab9ced284dcf8"

print(repr(revocation_policy["Content-Disposition"]))
assert revocation_policy["Content-Disposition"] == 'attachment; filename="widerrufsbelehrung.pdf"'
assert revocation_policy["Content-Type"] == "application/pdf"
hash = hashlib.sha256(decode_mail_part(revocation_policy)).hexdigest()
assert hash == "046c90a8d66a67acbb6e5154b83a3b61ef3b17ec0f4a91bea189b1c5d1076d74"

assert open("/var/lib/academy/invoices/R0000001.pdf", "rb").read() == invoice_pdf
