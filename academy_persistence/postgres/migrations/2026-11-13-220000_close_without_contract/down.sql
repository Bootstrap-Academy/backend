-- Refuse rollback while the new evidence exists; never relabel or discard it.
ALTER TABLE contract_processing_actions DROP CONSTRAINT contract_processing_actions_action_check;
ALTER TABLE contract_processing_actions ADD CONSTRAINT contract_processing_actions_action_check
CHECK(action IN ('record_external_resolution','schedule_premium_cancellation','legacy_unknown'));
