-- Prefer rolling back the application while retaining this additive column.
-- Removing it discards replay protection; wait out the entire accepted TOTP
-- window (90 seconds) with MFA writes stopped before reverting the migration.
alter table totp_device_secrets drop column last_accepted_step;
