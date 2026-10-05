-- Refuse schema rollback while operator entries exist. Application rollback
-- must retain this column definition; deleting audit evidence is never safe.
ALTER TABLE admin_audit_log ALTER COLUMN admin_user_id SET NOT NULL;
