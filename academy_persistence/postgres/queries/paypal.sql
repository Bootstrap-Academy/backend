--: CoinOrder(captured_at?, withdrawal_consent_at?, withdrawal_text_version?)

--! create_coin_order (captured_at?, withdrawal_consent_at?, withdrawal_text_version?)
insert into paypal_coin_orders (id, user_id, created_at, captured_at, coins, invoice_number, withdrawal_consent_at, withdrawal_text_version)
  values (:id, :user_id, :created_at, :captured_at, :coins, :invoice_number, :withdrawal_consent_at, :withdrawal_text_version);

--! count_coin_orders
select count(*) from paypal_coin_orders;

--! list_coin_orders : CoinOrder
select * from paypal_coin_orders;

--! list_coin_orders_by_user_id : CoinOrder
select * from paypal_coin_orders where user_id=:user_id order by created_at;

--! get_coin_order : CoinOrder
select * from paypal_coin_orders where id=:id;

--! get_coin_order_by_invoice_number : CoinOrder
select * from paypal_coin_orders where invoice_number=:invoice_number;

--! capture_coin_order
update paypal_coin_orders set captured_at=:captured_at where id=:id;

--! get_next_invoice_number
select nextval('invoice_number');
