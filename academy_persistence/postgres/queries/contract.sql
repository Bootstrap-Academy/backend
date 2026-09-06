--: ContractDeclaration (user_id?, cancellation_type?, requested_end?, effective_end?, processed_at?)

--! create (user_id?, cancellation_type?, requested_end?, effective_end?, processed_at?)
insert into contract_declarations (id, kind, received_at, name, email, user_id, contract, cancellation_type, details, requested_end, effective_end, processed_at)
  values (:id, :kind, :received_at, :name, :email, :user_id, :contract, :cancellation_type, :details, :requested_end, :effective_end, :processed_at);

--! list (kind?) : ContractDeclaration
select * from contract_declarations
  where (:kind::contract_declaration_kind is null or kind = :kind)
  order by received_at desc
  limit :limit offset :offset;

--! list_by_user_id : ContractDeclaration
select * from contract_declarations
  where user_id=:user_id
  order by received_at;

--! count (kind?)
select count(*) from contract_declarations
  where (:kind::contract_declaration_kind is null or kind = :kind);
