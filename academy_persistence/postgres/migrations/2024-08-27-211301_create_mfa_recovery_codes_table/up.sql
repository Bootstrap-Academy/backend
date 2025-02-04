create table mfa_recovery_codes (
    user_id uuid primary key references users(id) on delete cascade,
    code bytea not null
);
