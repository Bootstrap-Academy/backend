--: Document(user_id?, customer_details?, coins?, net_total_cents?, vat_total_cents?, gross_total_cents?, settled_at?, withdrawal_consent_at?, withdrawal_text_version?)

--! get_document : Document
select * from financial_documents where number=:number;

--! record_document (user_id?, customer_details?, coins?, net_total_cents?, vat_total_cents?, gross_total_cents?, withdrawal_consent_at?, withdrawal_text_version?)
insert into financial_documents (number, kind, user_id, issued_at, customer_details, coins, net_total_cents, vat_total_cents, gross_total_cents, withdrawal_consent_at, withdrawal_text_version)
  values (:number, :kind, :user_id, :issued_at, :customer_details, :coins, :net_total_cents, :vat_total_cents, :gross_total_cents, :withdrawal_consent_at, :withdrawal_text_version)
  on conflict (number) do update set
    user_id=coalesce(financial_documents.user_id, excluded.user_id),
    customer_details=coalesce(financial_documents.customer_details, excluded.customer_details),
    coins=coalesce(financial_documents.coins, excluded.coins),
    net_total_cents=coalesce(financial_documents.net_total_cents, excluded.net_total_cents),
    vat_total_cents=coalesce(financial_documents.vat_total_cents, excluded.vat_total_cents),
    gross_total_cents=coalesce(financial_documents.gross_total_cents, excluded.gross_total_cents),
    withdrawal_consent_at=coalesce(financial_documents.withdrawal_consent_at, excluded.withdrawal_consent_at),
    withdrawal_text_version=coalesce(financial_documents.withdrawal_text_version, excluded.withdrawal_text_version);

--! settle_document
update financial_documents set settled_at=:settled_at where number=:number;

--! pseudonymize_documents
update financial_documents set customer_details=:customer_details
  where user_id=:user_id and kind<>'final_statement';

--! list_documents_by_user_id : Document
select * from financial_documents where user_id=:user_id order by issued_at asc, number asc;

--! list_documents (kind?, search?) : Document
select * from financial_documents
  where (:kind::text is null or kind=:kind)
    and (:search::text is null
         or number ilike '%' || :search || '%'
         or coalesce(array_to_string(customer_details, ' '), '') ilike '%' || :search || '%')
  order by issued_at desc, number desc
  limit :limit offset :offset;

--! count_documents (kind?, search?)
select count(*) from financial_documents
  where (:kind::text is null or kind=:kind)
    and (:search::text is null
         or number ilike '%' || :search || '%'
         or coalesce(array_to_string(customer_details, ' '), '') ilike '%' || :search || '%');

--! list_document_numbers
select number from financial_documents order by number asc;

--! list_documents_issued_before : Document
select * from financial_documents d where issued_at<:issued_before
  and not exists(select 1 from commercial_document_holds h where h.number=d.number)
  and (kind<>'final_statement' or exists(select 1 from commercial_statement_disposal_reviews r where r.number=d.number and r.authorized))
  and (kind<>'invoice' or not commercial_invoice_identity_pending(d.number))
  order by issued_at asc;

--! delete_documents_issued_before
delete from financial_documents d where issued_at<:issued_before
  and not exists(select 1 from commercial_document_holds h where h.number=d.number)
  and (kind<>'final_statement' or exists(select 1 from commercial_statement_disposal_reviews r where r.number=d.number and r.authorized));
