import os
import subprocess

from utils import c, create_verified_account

RETENTION_MARKER = "Gelöschtes Konto (Aufbewahrung nach § 147 Abs. 3 AO)"
INVOICE = "/var/lib/academy/invoices/R0000001.pdf"


def query(sql):
    status, out = subprocess.getstatusoutput(f"sudo -u postgres psql -t --csv academy <<< '{sql}'")
    assert status == 0, out
    return out.strip()


def prune():
    assert os.system("systemctl start academy-task-prune-documents.service") == 0


a = create_verified_account("a", "a@a", "a")
resp = c.patch("/auth/users/me", json={"business": False, "country": "Germany"})
assert resp.status_code == 200

order_id = c.post(
    "/shop/coins/paypal/orders", json={"coins": 1337, "withdrawal_consent": True, "withdrawal_text_version": "2026-09"}
).json()
c.post(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}/confirm-payment-source")
resp = c.post(f"/shop/coins/paypal/orders/{order_id}/capture")
assert resp.status_code == 200

# The invoice is archived and recorded with the details it was issued with.
assert os.path.exists(INVOICE)
assert query("select number,kind,coins,gross_total_cents from financial_documents") == "R0000001,invoice,1337,1337"
assert query("select user_id from financial_documents") == a["user"]["id"]
assert "a@a" in query("select customer_details from financial_documents")

# Deleting the account keeps the document, but it no longer names the account.
resp = c.delete("/auth/users/me")
assert resp.status_code == 200

assert os.path.exists(INVOICE)
assert query("select user_id from financial_documents") == ""
assert RETENTION_MARKER in query("select customer_details from financial_documents")
assert "a@a" not in query("select customer_details from financial_documents")
assert query("select number,kind,coins,gross_total_cents from financial_documents") == "R0000001,invoice,1337,1337"
assert query("select count(*) from paypal_coin_orders") == "0"

# The retention period of a document issued in 2024 ends with 2032.
os.system("date -s '2032-12-31 12:00:00'")
prune()
assert os.path.exists(INVOICE)
assert query("select count(*) from financial_documents") == "1"

os.system("date -s '2033-01-01 12:00:00'")
prune()
assert not os.path.exists(INVOICE)
assert query("select count(*) from financial_documents") == "0"
