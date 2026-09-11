-- No cascading user FK: replay protection is financial evidence and survives deletion.
create table internal_coin_operations (
    id uuid primary key,
    user_id uuid not null,
    coins bigint not null,
    description text,
    credit_note boolean not null,
    balance bigint check (balance >= 0),
    withheld_balance bigint check (withheld_balance >= 0),
    created_at timestamptz not null default now(),
    completed_at timestamptz,
    check ((completed_at is null and balance is null and withheld_balance is null)
        or (completed_at is not null and balance is not null and withheld_balance is not null))
);
