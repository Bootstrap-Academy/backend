create table paypal_coin_orders (
    id text primary key,
    user_id uuid not null references users(id) on delete cascade,
    created_at timestamp with time zone not null,
    captured_at timestamp with time zone,
    coins bigint not null check (coins >= 0),
    invoice_number bigint not null check (invoice_number >= 1)
);
