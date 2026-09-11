-- Refuse rather than discard any live incarnation or recorded review history.
LOCK TABLE commercial_document_holds,commercial_contract_holds,commercial_renewal_holds,commercial_legacy_renewal_holds IN ACCESS EXCLUSIVE MODE;
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM commercial_document_holds) OR EXISTS(SELECT 1 FROM commercial_contract_holds)
  OR EXISTS(SELECT 1 FROM commercial_renewal_holds) OR EXISTS(SELECT 1 FROM commercial_legacy_renewal_holds)
  OR EXISTS(SELECT 1 FROM commercial_journal WHERE kind='hold_review') THEN
  RAISE EXCEPTION 'Unsafe hold-review downgrade: preserve incarnation, revision and journal evidence';
 END IF;
END $$;
DROP FUNCTION commercial_operation(text,uuid,jsonb);
ALTER FUNCTION commercial_operation_before_hold_review(text,uuid,jsonb) RENAME TO commercial_operation;
DROP FUNCTION commercial_hold_operation(text,uuid,jsonb);
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['commercial_document_holds','commercial_contract_holds','commercial_renewal_holds','commercial_legacy_renewal_holds'] LOOP
  EXECUTE format('DROP TRIGGER commercial_hold_insert ON %I',t);
  EXECUTE format('DROP TRIGGER commercial_hold_update ON %I',t);
  EXECUTE format('ALTER TABLE %I DROP COLUMN review_command_id, DROP COLUMN review_version, DROP COLUMN incarnation_id',t);
 END LOOP;
END $$;
DROP FUNCTION commercial_hold_insert();
DROP FUNCTION commercial_hold_update();
DROP FUNCTION commercial_hold_last_review(jsonb,jsonb,uuid);
