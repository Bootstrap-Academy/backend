-- Both tables are read by user id on every data export and, for the coin
-- orders, on every account deletion. A foreign key does not create an index in
-- Postgres, so those reads were sequential scans. The two tables that gained a
-- per-user query in the same wave, `withdrawal_consents` and
-- `financial_documents`, already have one.
create index paypal_coin_orders_user_id_idx on paypal_coin_orders (user_id);
create index contract_declarations_user_id_idx on contract_declarations (user_id);
