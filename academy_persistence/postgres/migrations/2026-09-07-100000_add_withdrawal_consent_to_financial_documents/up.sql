-- The declarations under § 356 Abs. 6 Nr. 2 BGB that a consumer gives before
-- buying Morphcoins are recorded in `withdrawal_consents` and on the order
-- row, and both are deleted together with the account. The invoice for that
-- order is kept for eight years, so the evidence that the declarations were
-- given disappeared long before the document they belong to.
--
-- The invoice record therefore keeps its own copy: it is written when the
-- invoice is issued and lives exactly as long as the invoice does.
alter table financial_documents
  add column withdrawal_consent_at timestamp with time zone,
  add column withdrawal_text_version text;

-- Invoices that were issued before this column existed take the declaration
-- from their order, as long as that order is still there.
update financial_documents
  set withdrawal_consent_at = paypal_coin_orders.withdrawal_consent_at,
      withdrawal_text_version = paypal_coin_orders.withdrawal_text_version
  from paypal_coin_orders
  where financial_documents.kind = 'invoice'
    and financial_documents.number = 'R' || lpad(paypal_coin_orders.invoice_number::text, 7, '0')
    and paypal_coin_orders.withdrawal_consent_at is not null;
