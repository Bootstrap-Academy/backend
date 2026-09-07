--: ContractDeclaration (user_id?, contract_designation?, cancellation_type?, requested_end?, effective_end?, processed_at?, processing_note?)

--! create (user_id?, contract_designation?, cancellation_type?, requested_end?, effective_end?, processed_at?, processing_note?)
insert into contract_declarations (id, kind, received_at, name, email, user_id, contract, contract_designation, cancellation_type, details, requested_end, effective_end, processed_at, processing_note)
  values (:id, :kind, :received_at, :name, :email, :user_id, :contract, :contract_designation, :cancellation_type, :details, :requested_end, :effective_end, :processed_at, :processing_note);

--! get : ContractDeclaration
select * from contract_declarations where id=:id;

--! list (kind?) : ContractDeclaration
select * from contract_declarations
  where (:kind::contract_declaration_kind is null or kind = :kind)
  order by received_at desc, id desc
  limit :limit offset :offset;

--! list_by_user_id : ContractDeclaration
select * from contract_declarations
  where user_id=:user_id
  order by received_at;

--! count (kind?)
select count(*) from contract_declarations
  where (:kind::contract_declaration_kind is null or kind = :kind);

--! set_processed (effective_end?, processing_note?) : ContractDeclaration
update contract_declarations
  set processed_at=:processed_at, effective_end=:effective_end, processing_note=:processing_note
  where id=:id
  returning *;

--! delete_by_received_at
delete from contract_declarations where received_at<:received_at;
