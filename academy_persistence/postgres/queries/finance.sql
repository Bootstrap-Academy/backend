--: Document(user_id?, customer_details?, coins?, net_total_cents?, vat_total_cents?, gross_total_cents?)

--! get_document : Document
select * from financial_documents where number=:number;

--! record_document (user_id?, customer_details?, coins?, net_total_cents?, vat_total_cents?, gross_total_cents?)
insert into financial_documents (number, kind, user_id, issued_at, customer_details, coins, net_total_cents, vat_total_cents, gross_total_cents)
  values (:number, :kind, :user_id, :issued_at, :customer_details, :coins, :net_total_cents, :vat_total_cents, :gross_total_cents)
  on conflict (number) do update set
    user_id=coalesce(financial_documents.user_id, excluded.user_id),
    customer_details=coalesce(financial_documents.customer_details, excluded.customer_details),
    coins=coalesce(financial_documents.coins, excluded.coins),
    net_total_cents=coalesce(financial_documents.net_total_cents, excluded.net_total_cents),
    vat_total_cents=coalesce(financial_documents.vat_total_cents, excluded.vat_total_cents),
    gross_total_cents=coalesce(financial_documents.gross_total_cents, excluded.gross_total_cents);

--! pseudonymize_documents
update financial_documents set customer_details=:customer_details where user_id=:user_id;

--! list_documents_issued_before : Document
select * from financial_documents where issued_at<:issued_before order by issued_at asc;

--! delete_documents_issued_before
delete from financial_documents where issued_at<:issued_before;
