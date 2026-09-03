-- Declarations under § 356 Abs. 5 Nr. 2 / Abs. 6 Nr. 2 BGB given at checkout.
--
-- Every consent is recorded in `withdrawal_consents`. Coin orders keep a copy
-- of the declaration on the order row as well, because the confirmation of the
-- contract (§ 312f Abs. 3 BGB) is sent when the order is captured, which can
-- happen long after the order was placed.
create table withdrawal_consents (
    id uuid primary key,
    user_id uuid not null references users(id) on delete cascade,
    subject text not null,
    reference text,
    text_version text not null,
    consented_at timestamp with time zone not null
);

create index withdrawal_consents_user_id_idx on withdrawal_consents (user_id);

alter table paypal_coin_orders
  add column withdrawal_consent_at timestamp with time zone,
  add column withdrawal_text_version text;
