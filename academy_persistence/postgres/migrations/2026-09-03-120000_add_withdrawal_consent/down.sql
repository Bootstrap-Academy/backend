alter table paypal_coin_orders
  drop column withdrawal_consent_at,
  drop column withdrawal_text_version;

drop index if exists withdrawal_consents_user_id_idx;

drop table withdrawal_consents;
