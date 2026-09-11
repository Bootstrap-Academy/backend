import os
import hashlib
from pathlib import Path
import subprocess

from utils import c, create_admin_account, create_verified_account, make_client, paypal_order
from utils import configure_purchases

configure_purchases()


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

order_id = paypal_order(1337, c)
c.post(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}/confirm-payment-source")
resp = c.post(f"/shop/coins/paypal/orders/{order_id}/capture")
assert resp.status_code == 200

# The invoice is archived and recorded with the details it was issued with.
assert os.path.exists(INVOICE)
assert query("select number,kind,coins,gross_total_cents from financial_documents") == "R0000001,invoice,1337,1337"
assert query("select user_id from financial_documents") == a["user"]["id"]
assert "a@a" in query("select customer_details from financial_documents")

# It also records the declarations under § 356 Abs. 6 Nr. 2 BGB that were given
# for the order, because the order itself goes with the account.
assert query("select withdrawal_text_version from financial_documents") == "L1-request-2026-09"
assert query("select withdrawal_consent_at is not null from financial_documents") == "t"

# The issued documents are part of the data export.
resp = c.get("/auth/users/me/export")
assert resp.status_code == 200
documents = resp.json()["account"]["financial_documents"]
assert [(d["number"], d["kind"], d["coins"]) for d in documents] == [("R0000001", "INVOICE", 1337)]

# Deleting the account removes its live link, preserving necessary original evidence.
original_invoice = Path(INVOICE).read_bytes()
original_archive = query("select encode(sha256(pdf), 'hex') from invoice_originals where invoice_number='R0000001'")
assert original_archive == hashlib.sha256(original_invoice).hexdigest()
resp = c.delete("/auth/users/me")
assert resp.status_code == 200

assert os.path.exists(INVOICE)
assert query("select user_id from financial_documents where kind='invoice'") == ""
# Search/list metadata is minimized independently of immutable original bytes.
assert RETENTION_MARKER in query("select customer_details from financial_documents where kind='invoice'")
assert "a@a" not in query("select customer_details from financial_documents where kind='invoice'")
assert (
    query("select encode(sha256(pdf), 'hex') from invoice_originals where invoice_number='R0000001'")
    == original_archive
)
assert Path(INVOICE).read_bytes() == original_invoice
assert (
    query("select number,kind,coins,gross_total_cents from financial_documents where kind='invoice'")
    == "R0000001,invoice,1337,1337"
)
assert query("select count(*) from paypal_coin_orders") == "0"

# The live order is gone; the exact acceptance revision stays with its invoice.
assert query("select withdrawal_text_version from financial_documents where kind='invoice'") == "L1-request-2026-09"
assert query("select withdrawal_consent_at is not null from financial_documents where kind='invoice'") == "t"

# The final statement records the unused share of the purchased Morphcoins and
# preserves the name and email alongside other necessary originals, so that the
# amount can still be refunded on request (AGB Ziffer 6.7).
statement_number = query("select number from financial_documents where kind='final_statement'")
assert statement_number == "S1"
final_statement = f"/var/lib/academy/final_statements/{statement_number}.pdf"
assert os.path.exists(final_statement)
assert query("select coins,gross_total_cents from financial_documents where kind='final_statement'") == "1337,1337"
assert query("select user_id from financial_documents where kind='final_statement'") == ""
details = query("select customer_details from financial_documents where kind='final_statement'")
assert "a@a" in details

# An administrator can find both documents, and the final statement by the
# email address it still carries.
resp = adm.get("/finance/documents")
assert resp.status_code == 200
listing = resp.json()
assert listing["total"] == 2
assert [d["number"] for d in listing["documents"]] == [statement_number, "R0000001"]
assert all(d["user_id"] is None for d in listing["documents"])

# The listing shows the declarations of the invoiced order; a final statement
# documents no order and carries none.
invoice_entry = next(d for d in listing["documents"] if d["number"] == "R0000001")
assert invoice_entry["withdrawal_text_version"] == "L1-request-2026-09"
assert invoice_entry["withdrawal_consent_at"] is not None
statement_entry = next(d for d in listing["documents"] if d["number"] == statement_number)
assert statement_entry["withdrawal_text_version"] is None
assert statement_entry["withdrawal_consent_at"] is None

resp = adm.get("/finance/documents", params={"search": "a@a"})
assert resp.status_code == 200
assert [d["number"] for d in resp.json()["documents"]] == [statement_number]

resp = adm.get("/finance/documents", params={"kind": "FINAL_STATEMENT"})
assert resp.status_code == 200
assert [d["number"] for d in resp.json()["documents"]] == [statement_number]
assert resp.json()["documents"][0]["settled_at"] is None


def statement():
    resp = adm.get("/finance/documents", params={"kind": "FINAL_STATEMENT"})
    assert resp.status_code == 200
    return resp.json()["documents"][0]


# Timestamp-only settlement is explicitly unavailable. A rejected command must
# not change the statement or authorize disposal of either original.
statement_before = statement()
for number in [statement_number, "R0000001", "S999"]:
    result = subprocess.run(["academy", "admin", "finance", "settle", number], capture_output=True, text=True)
    assert result.returncode != 0
    assert "Settlement execution is not available" in result.stderr
    assert statement() == statement_before
    assert Path(INVOICE).read_bytes() == original_invoice

# The listing requires admin privileges.
resp = c.get("/finance/documents")
assert resp.status_code == 401

# An account that spent everything it bought gets no final statement: there is
# nothing left to refund, so there is no reason to keep a document that names
# it.
b = make_client()
b_login = create_verified_account("b", "b@b", "b", b)
resp = b.patch("/auth/users/me", json={"business": False, "country": "Germany"})
assert resp.status_code == 200

order_id = paypal_order(500, b)
b.post(f"http://127.0.0.1:8103/v2/checkout/orders/{order_id}/confirm-payment-source")
resp = b.post(f"/shop/coins/paypal/orders/{order_id}/capture")
assert resp.status_code == 200

b_id = b_login["user"]["id"]
resp = adm.post(f"/shop/coins/{b_id}", json={"coins": -500, "description": "spent", "credit_note": False})
assert resp.status_code == 200

resp = b.delete("/auth/users/me")
assert resp.status_code == 200

# The second issued invoice remains necessary original evidence; there is no
# second final statement because its purchased balance was spent.
assert query("select count(*) from financial_documents where kind='final_statement'") == "1"
assert query("select count(*) from financial_documents where kind='invoice'") == "2"
assert "b@b" not in query("select array_to_string(customer_details, ' ') from financial_documents")


# Calendar expiry alone cannot erase retained originals or settle remaining
# claims. The installed forward guard is exercised, never removed for this test.
def originals():
    paths = [Path(INVOICE), Path(final_statement), Path("/var/lib/academy/invoices/R0000002.pdf")]
    return {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}


saved_files = originals()
saved_archives = query(
    "select jsonb_agg(jsonb_build_array(invoice_number, encode(sha256(pdf), 'hex')) order by invoice_number) from invoice_originals"
)
saved_documents = query("select jsonb_agg(to_jsonb(d) order by number) from financial_documents d")
for at in ["2032-12-31 12:00:00", "2033-01-01 12:00:00"]:
    subprocess.run(["date", "-s", at], check=True)
    prune()
    assert originals() == saved_files
    assert (
        query(
            "select jsonb_agg(jsonb_build_array(invoice_number, encode(sha256(pdf), 'hex')) order by invoice_number) from invoice_originals"
        )
        == saved_archives
    )
    assert query("select jsonb_agg(to_jsonb(d) order by number) from financial_documents d") == saved_documents
    assert query("select count(*) from financial_documents") == "3"

# Kept files are accounted for, not orphaned merely because their users are gone.
status, out = subprocess.getstatusoutput("academy task list-orphan-documents")
assert status == 0, out
assert "/var/lib/academy" not in out, out
