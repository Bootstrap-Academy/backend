import os
import subprocess

from utils import c, create_admin_account, create_verified_account, make_client

RETENTION_MARKER = "Gelöschtes Konto (Aufbewahrung nach § 147 Abs. 3 AO)"
INVOICE = "/var/lib/academy/invoices/R0000001.pdf"


def query(sql):
    # A quoted heredoc, so that the statement may contain quotes itself.
    status, out = subprocess.getstatusoutput(f"sudo -u postgres psql -t --csv academy <<'SQL'\n{sql}\nSQL")
    assert status == 0, out
    return out.strip()


def prune():
    assert os.system("systemctl start academy-task-prune-documents.service") == 0


adm = make_client()
create_admin_account("adm", "adm@example.com", "adm", adm)

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

# The issued documents are part of the data export.
resp = c.get("/auth/users/me/export")
assert resp.status_code == 200
documents = resp.json()["account"]["financial_documents"]
assert [(d["number"], d["kind"], d["coins"]) for d in documents] == [("R0000001", "INVOICE", 1337)]

# Deleting the account keeps the document, but it no longer names the account.
resp = c.delete("/auth/users/me")
assert resp.status_code == 200

assert os.path.exists(INVOICE)
assert query("select user_id from financial_documents where kind='invoice'") == ""
assert RETENTION_MARKER in query("select customer_details from financial_documents where kind='invoice'")
assert "a@a" not in query("select customer_details from financial_documents where kind='invoice'")
assert (
    query("select number,kind,coins,gross_total_cents from financial_documents where kind='invoice'")
    == "R0000001,invoice,1337,1337"
)
assert query("select count(*) from paypal_coin_orders") == "0"

# The final statement records the unused share of the purchased Morphcoins and
# is the one document that keeps the name and the email address, so that the
# amount can still be refunded on request (AGB Ziffer 6.7).
statement_number = query("select number from financial_documents where kind='final_statement'")
assert statement_number == "S1"
final_statement = f"/var/lib/academy/final_statements/{statement_number}.pdf"
assert os.path.exists(final_statement)
assert query("select coins,gross_total_cents from financial_documents where kind='final_statement'") == "1337,1337"
assert query("select user_id from financial_documents where kind='final_statement'") == ""
details = query("select customer_details from financial_documents where kind='final_statement'")
assert "a@a" in details
assert RETENTION_MARKER not in details

# An administrator can find both documents, and the final statement by the
# email address it still carries.
resp = adm.get("/finance/documents")
assert resp.status_code == 200
listing = resp.json()
assert listing["total"] == 2
assert [d["number"] for d in listing["documents"]] == [statement_number, "R0000001"]
assert all(d["user_id"] is None for d in listing["documents"])

resp = adm.get("/finance/documents", params={"search": "a@a"})
assert resp.status_code == 200
assert [d["number"] for d in resp.json()["documents"]] == [statement_number]

resp = adm.get("/finance/documents", params={"kind": "FINAL_STATEMENT"})
assert resp.status_code == 200
assert [d["number"] for d in resp.json()["documents"]] == [statement_number]

# The listing requires admin privileges.
resp = c.get("/finance/documents")
assert resp.status_code == 401

# The retention period of a document issued in 2024 ends with 2032.
os.system("date -s '2032-12-31 12:00:00'")
prune()
assert os.path.exists(INVOICE)
assert os.path.exists(final_statement)
assert query("select count(*) from financial_documents") == "2"

os.system("date -s '2033-01-01 12:00:00'")
prune()
assert not os.path.exists(INVOICE)
assert not os.path.exists(final_statement)
assert query("select count(*) from financial_documents") == "0"

# Nothing is left behind that the orphan report could complain about.
status, out = subprocess.getstatusoutput("academy task list-orphan-documents")
assert status == 0, out
assert "/var/lib/academy" not in out, out
