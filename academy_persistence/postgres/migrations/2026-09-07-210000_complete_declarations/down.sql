DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM contract_delivery) OR EXISTS (SELECT 1 FROM contract_receipt_access) OR EXISTS (SELECT 1 FROM contract_cancellation_schedule) THEN
    RAISE EXCEPTION 'Retained declaration evidence requires forward repair';
  END IF;
END $$;
DROP TRIGGER observe_declared_premium ON premium;
DROP FUNCTION observe_declared_premium_period();
DROP TABLE contract_period_observation, contract_account_observation;
DROP TRIGGER immutable_declaration ON contract_declarations;
DROP TABLE contract_cancellation_schedule, contract_delivery, contract_receipt_access;
DROP FUNCTION protect_contract_evidence();
