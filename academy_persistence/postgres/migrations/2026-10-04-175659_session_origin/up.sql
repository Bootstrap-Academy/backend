-- Who signed in to a session. Additive and instant: no session ends, and no row
-- is rewritten or updated, so the row triggers on sessions do not fire.
--
-- Existing sessions never recorded their origin and become 'legacy'. The same
-- default applies to any insert that names no origin, for example by an older
-- backend after an application rollback. The backend treats 'legacy' sessions
-- conservatively: only those with a device name count as the owner's own
-- sign-in, because signing in to someone else's account has never set one.
ALTER TABLE sessions
 ADD COLUMN origin text NOT NULL DEFAULT 'legacy'
  CHECK (origin IN ('sign_in','impersonation','legacy')),
 -- The administrator who signed in to the account. Their sessions in other
 -- accounts end together with the administrator's own account.
 ADD COLUMN impersonated_by uuid REFERENCES users(id) ON DELETE CASCADE,
 ADD CONSTRAINT sessions_impersonated_by_origin
  CHECK (impersonated_by IS NULL OR origin='impersonation');

CREATE INDEX sessions_impersonated_by_idx ON sessions (impersonated_by)
 WHERE impersonated_by IS NOT NULL;
