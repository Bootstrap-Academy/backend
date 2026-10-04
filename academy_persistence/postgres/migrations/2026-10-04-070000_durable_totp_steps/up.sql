alter table totp_device_secrets
    add column last_accepted_step bigint not null default -1
    check (last_accepted_step >= -1);
