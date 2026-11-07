-- Lapsed obligations stay rejected with their journal; only the operation goes.
CREATE OR REPLACE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation='admin_retention_page' THEN RETURN commercial_retention_page(p_actor,p_body); END IF;
 RETURN commercial_operation_before_retention_page(p_operation,p_actor,p_body);
END $$;
DROP FUNCTION commercial_forfeit_unpaid_remainder(uuid,jsonb);
