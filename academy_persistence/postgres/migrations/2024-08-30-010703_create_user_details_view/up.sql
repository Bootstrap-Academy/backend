drop view if exists user_details;
create view user_details as (
    select
        u.id as user_id,
        (exists (select td.id from totp_devices td where td.user_id=u.id and td.enabled)) as mfa_enabled
    from users u
);
