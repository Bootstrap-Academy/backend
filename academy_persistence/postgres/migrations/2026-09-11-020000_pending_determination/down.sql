-- Restore the complete LC1 predecessor; preserve every decision and hold row.
DROP FUNCTION commercial_operation(text,uuid,jsonb);
ALTER FUNCTION commercial_operation_before_pending_determination(text,uuid,jsonb) RENAME TO commercial_operation;
DROP FUNCTION commercial_determination_status(uuid,jsonb);
DROP FUNCTION commercial_determination_journal(commercial_journal);
DROP FUNCTION commercial_pending_determination(uuid,jsonb);
