-- Dropping origin columns while a delegated session exists would erase its
-- provenance. The migration CLI must refuse that rollback, preserving sessions.
DO $$
BEGIN
 IF EXISTS (SELECT 1 FROM sessions WHERE origin='impersonation') THEN
  RAISE EXCEPTION 'Cannot remove provenance while impersonation sessions exist';
 END IF;
END $$;

-- Prefer rolling back the application while keeping these columns: an older
-- backend leaves new sessions at 'legacy'. Removing them forgets which open
-- sessions an administrator started, and those count as the owner's again.
ALTER TABLE sessions
 DROP COLUMN impersonated_by,
 DROP COLUMN origin;
