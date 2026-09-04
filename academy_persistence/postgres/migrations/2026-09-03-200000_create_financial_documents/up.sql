-- Invoices and credit notes have to be kept for eight years, counted from the
-- end of the calendar year in which they were issued (§ 147 Abs. 3 Satz 1 und
-- Abs. 4 AO, § 257 Abs. 4 HGB, § 14b Abs. 1 UStG).
--
-- This table records every document that has been issued so that
--   * the retention period can be enforced (`academy task prune-documents`),
--   * a document keeps the content it was issued with, and
--   * the record survives the deletion of the account it was issued for. The
--     account reference is dropped and the customer details are replaced by a
--     retention marker, while number, amounts and dates are kept.
create table financial_documents (
    number text primary key,
    kind text not null check (kind in ('invoice', 'credit_note')),
    user_id uuid references users(id) on delete set null,
    issued_at timestamp with time zone not null,
    -- Address block as printed on the document, one line per entry.
    customer_details text[],
    coins bigint check (coins >= 0),
    -- Totals in euro cents, rounded exactly as they are printed.
    net_total_cents bigint,
    vat_total_cents bigint,
    gross_total_cents bigint
);

create index financial_documents_user_id_idx on financial_documents (user_id);
create index financial_documents_issued_at_idx on financial_documents (issued_at);

-- Every coin order is assigned an invoice number when the order is created.
-- One hundred Morphcoins correspond to one Euro, so the gross total in cents
-- equals the number of Morphcoins. Customer details and the net/vat split are
-- only recorded for documents issued from now on.
insert into financial_documents (number, kind, user_id, issued_at, coins, gross_total_cents)
  select 'R' || lpad(invoice_number::text, 7, '0'), 'invoice', user_id, created_at, coins, coins
  from paypal_coin_orders;
