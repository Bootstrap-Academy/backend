--: TotpDevice()

--! list_totp_devices_by_user : TotpDevice
select * from totp_devices where user_id=:user_id;

--! create_totp_device
insert into totp_devices (id, user_id, enabled, created_at)
  values (:id, :user_id, :enabled, :created_at);

--! update_totp_device (enabled?)
update totp_devices
  set enabled=coalesce(:enabled, enabled)
  where id=:id;

--! delete_totp_devices_by_user
delete from totp_devices where user_id=:user_id;

--! list_enabled_totp_device_secrets_by_user
select secret from totp_device_secrets
  inner join totp_devices using(id)
  where user_id=:user_id and enabled;

--! get_totp_device_secret
select secret from totp_device_secrets where id=:id;

--! set_totp_device_secret
insert into totp_device_secrets (id, secret) values (:id, :secret)
  on conflict (id) do update set secret=:secret;

--! get_recovery_code_hash
select code from mfa_recovery_codes where user_id=:user_id;

--! set_recovery_code_hash
insert into mfa_recovery_codes (user_id, code) values (:user_id, :code)
  on conflict (user_id) do update set code=:code;

--! delete_recovery_code_hash
delete from mfa_recovery_codes where user_id=:user_id;
