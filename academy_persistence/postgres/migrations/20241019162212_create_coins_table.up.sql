create table coins (
    user_id uuid primary key references users(id) on delete cascade,
    coins bigint not null check (coins >= 0),
    withheld_coins bigint not null check (withheld_coins >= 0)
);
