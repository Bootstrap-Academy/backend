create table hearts (
    user_id uuid primary key references users(id) on delete cascade,
    hearts bigint not null check (hearts >= 0),
    last_refill timestamp with time zone not null
);
