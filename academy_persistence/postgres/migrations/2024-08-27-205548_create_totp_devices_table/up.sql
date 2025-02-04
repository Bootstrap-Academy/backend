create table totp_devices (
    id uuid primary key,
    user_id uuid unique not null references users(id) on delete cascade,
    enabled boolean not null,
    created_at timestamp with time zone not null
);

create table totp_device_secrets (
    id uuid primary key references totp_devices(id) on delete cascade,
    secret bytea not null
);

alter table totp_devices add foreign key (id) references totp_device_secrets(id) initially deferred;
