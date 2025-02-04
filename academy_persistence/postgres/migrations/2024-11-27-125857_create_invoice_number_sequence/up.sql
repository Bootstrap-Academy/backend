create sequence invoice_number start with 1;
select setval('invoice_number', (select coalesce(max(invoice_number), 0) + 1 from paypal_coin_orders), false);
