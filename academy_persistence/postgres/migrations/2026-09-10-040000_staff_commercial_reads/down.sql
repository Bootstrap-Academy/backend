-- Remove only this read, retaining the immediate effective IF1 dispatcher.
DROP FUNCTION commercial_operation(text,uuid,jsonb);
ALTER FUNCTION commercial_operation_before_staff_reads(text,uuid,jsonb) RENAME TO commercial_operation;
DROP FUNCTION commercial_admin_cash_capacity(uuid,uuid);
