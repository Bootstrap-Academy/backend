create table transactions (
    id uuid primary key,
    user_id uuid not null references users(id) on delete cascade,
    created_at timestamp with time zone not null,
    coins bigint not null,
    description text,
    include_in_credit_note boolean not null
);
