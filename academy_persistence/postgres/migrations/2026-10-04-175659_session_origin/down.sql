-- Prefer rolling back the application while keeping these columns: an older
-- backend leaves new sessions at 'legacy'. Removing them forgets which open
-- sessions an administrator started, and those count as the owner's again.
ALTER TABLE sessions
 DROP COLUMN impersonated_by,
 DROP COLUMN origin;
