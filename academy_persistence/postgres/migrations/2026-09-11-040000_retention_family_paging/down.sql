-- Restore exactly the accepted determination dispatcher; invoice protection stays installed.
DROP FUNCTION commercial_operation(text,uuid,jsonb);
ALTER FUNCTION commercial_operation_before_retention_page(text,uuid,jsonb) RENAME TO commercial_operation;
DROP FUNCTION commercial_retention_page(uuid,jsonb);
DROP FUNCTION commercial_retention_page_projection(text,integer,jsonb);
DROP FUNCTION commercial_retention_page_keys(jsonb,text[]);

