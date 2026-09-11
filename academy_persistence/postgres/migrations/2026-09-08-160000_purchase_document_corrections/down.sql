DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM purchase_document_corrections) OR EXISTS(SELECT 1 FROM purchase_fulfillments WHERE statement_version<>1) OR EXISTS(SELECT 1 FROM purchase_provision_observations WHERE statement_version<>1) THEN
  RAISE EXCEPTION 'Cannot discard retained document corrections or statement version evidence';
 END IF;
END $$;
DROP TRIGGER correct_legacy_purchase_fulfillment ON purchase_fulfillments;
DROP TRIGGER correct_legacy_purchase_timing ON purchase_provision_observations;
DROP FUNCTION correct_legacy_purchase_document();
DROP FUNCTION correct_purchase_document(uuid,text,text,jsonb);
DROP TABLE purchase_document_corrections;
ALTER TABLE purchase_fulfillments DROP COLUMN statement_version;
ALTER TABLE purchase_provision_observations DROP COLUMN statement_version;
