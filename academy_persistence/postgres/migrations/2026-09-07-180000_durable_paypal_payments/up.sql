-- Payment identity, provider proof and fulfillment evidence must survive account deletion.
create table paypal_payments (
    order_id text primary key,
    invoice_number bigint not null unique,
    user_id uuid not null,
    request_id uuid not null unique,
    snapshot text not null,
    started_at timestamptz,
    attempts bigint not null default 0 check (attempts >= 0),
    capture_id text unique,
    capture text,
    balance bigint check (balance >= 0),
    withheld_balance bigint check (withheld_balance >= 0),
    fulfilled_at timestamptz,
    receipt_sent_at timestamptz,
    receipt_attempts bigint not null default 0 check (receipt_attempts >= 0),
    last_error text,
    check ((capture_id is null) = (capture is null)),
    check ((fulfilled_at is null and balance is null and withheld_balance is null)
        or (fulfilled_at is not null and balance is not null and withheld_balance is not null and capture_id is not null)),
    check (receipt_sent_at is null or fulfilled_at is not null)
);
create index paypal_payments_pending on paypal_payments (started_at)
    where started_at is not null and receipt_sent_at is null;
create function protect_paypal_payment() returns trigger language plpgsql as $$
begin
    if (new.order_id, new.invoice_number, new.user_id, new.request_id, new.snapshot)
        is distinct from (old.order_id, old.invoice_number, old.user_id, old.request_id, old.snapshot)
        or (old.started_at is not null and new.started_at is distinct from old.started_at)
        or (old.capture is not null and (new.capture, new.capture_id) is distinct from (old.capture, old.capture_id))
        or (old.fulfilled_at is not null and (new.fulfilled_at, new.balance, new.withheld_balance)
            is distinct from (old.fulfilled_at, old.balance, old.withheld_balance))
        or (old.receipt_sent_at is not null and new.receipt_sent_at is distinct from old.receipt_sent_at)
        or new.attempts < old.attempts or new.receipt_attempts < old.receipt_attempts then
        raise exception 'PayPal payment evidence is immutable';
    end if;
    return new;
end $$;
create trigger paypal_payment_evidence before update on paypal_payments
    for each row execute function protect_paypal_payment();

-- Existing uncaptured local rows can represent remote captures lost by the old transaction.
-- Retain only facts that actually exist; no paid outcome, merchant, price or tax is inferred.
create table paypal_legacy_reconciliation (
    order_id text primary key,
    user_id uuid not null,
    coins bigint not null,
    invoice_number bigint not null,
    recorded_at timestamptz not null default now(),
    order_snapshot text not null
);
insert into paypal_legacy_reconciliation (order_id,user_id,coins,invoice_number,order_snapshot)
    select id,user_id,coins,invoice_number,row_to_json(o)::text
    from paypal_coin_orders o where captured_at is null;
