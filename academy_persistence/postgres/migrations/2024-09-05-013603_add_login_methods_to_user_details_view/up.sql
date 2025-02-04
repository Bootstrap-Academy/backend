drop view if exists user_details;
create view user_details as (
    select
        u.id as user_id,
        (exists (select td.id from totp_devices td where td.user_id=u.id and td.enabled)) as mfa_enabled,
        (exists (select up.user_id from user_passwords up where up.user_id=u.id)) as password_login,
        (exists (select ol.user_id from oauth2_links ol where ol.user_id=u.id)) as oauth2_login
    from users u
);
