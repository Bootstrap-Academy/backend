--: Consent(reference?)

--! create (reference?)
insert into withdrawal_consents (id, user_id, subject, reference, text_version, consented_at)
  values (:id, :user_id, :subject, :reference, :text_version, :consented_at);

--! list_by_user_id : Consent
select * from withdrawal_consents where user_id=:user_id order by consented_at asc;
