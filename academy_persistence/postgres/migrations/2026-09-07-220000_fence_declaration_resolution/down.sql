DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM premium_transaction_observation) OR EXISTS(SELECT 1 FROM premium_period_changes) OR EXISTS(SELECT 1 FROM contract_premium_operations) OR EXISTS(SELECT 1 FROM contract_processing_actions) OR EXISTS(SELECT 1 FROM contract_delivery WHERE superseded_at IS NOT NULL OR kind='external_resolution') THEN
   RAISE EXCEPTION 'Retained period/action/delivery evidence requires forward repair';
 END IF;
END $$;
DROP TRIGGER observe_premium_transaction ON transactions;
DROP FUNCTION observe_premium_transaction();
DROP TABLE premium_transaction_observation;
DROP TRIGGER record_premium_period ON premium;
DROP FUNCTION record_premium_period_change();
DROP TABLE contract_premium_operations,premium_period_changes,contract_processing_actions;
ALTER TABLE contract_delivery DROP COLUMN superseded_at;
ALTER TABLE contract_delivery DROP CONSTRAINT contract_delivery_kind_check;
ALTER TABLE contract_delivery ADD CONSTRAINT contract_delivery_kind_check CHECK(kind IN ('receipt','resolution','internal'));
