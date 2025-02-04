create table user_invoice_info (
    user_id uuid primary key references users(id) on delete cascade,
    business boolean,
    first_name text,
    last_name text,
    street text,
    zip_code text,
    city text,
    country text,
    vat_id text
);

insert into user_invoice_info (user_id) select id from users;

alter table users add foreign key (id) references user_invoice_info(user_id) initially deferred;
