create sequence user_number start with 1;
create table user_numbers (
    user_id uuid primary key references users(id) on delete cascade,
    number bigint unique not null
);
