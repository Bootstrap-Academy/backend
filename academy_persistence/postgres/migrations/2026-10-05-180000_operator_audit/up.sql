-- CLI impersonation has no known administrator account. Preserve that fact
-- instead of dropping its requests or attributing them to the account owner.
ALTER TABLE admin_audit_log ALTER COLUMN admin_user_id DROP NOT NULL;
