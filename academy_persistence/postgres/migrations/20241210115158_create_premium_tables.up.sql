create table premium (
    id uuid primary key,
    user_id uuid not null references users(id) on delete cascade,
    since timestamp with time zone not null,
    until timestamp with time zone not null
);

create table premium_subscriptions (
    user_id uuid primary key references users(id) on delete cascade,
    plan smallint not null
);
