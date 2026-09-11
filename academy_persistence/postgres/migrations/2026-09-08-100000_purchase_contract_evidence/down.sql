-- Retained accepted contracts must be archived before any deliberate downgrade.
DO $$ BEGIN
 IF EXISTS (SELECT 1 FROM purchase_submissions) OR EXISTS (SELECT 1 FROM purchase_acceptances) OR EXISTS (SELECT 1 FROM invoice_originals) OR EXISTS (SELECT 1 FROM invoice_reconciliation) THEN
   RAISE EXCEPTION 'Cannot discard purchase or invoice reconciliation evidence';
 END IF;
END $$;
ALTER TABLE premium_period_changes DROP COLUMN purchase_evidence;
DROP TRIGGER observe_new_purchase_debit ON purchase_debits;
DROP FUNCTION observe_new_purchase_debit();
DROP FUNCTION observe_contract_purchases(uuid,uuid);
DROP TABLE paypal_contract_orders,paypal_receipt_observations,paypal_receipt_artifacts,purchase_cash_captures,contract_purchase_observations, purchase_delivery_attempts, purchase_debits, invoice_originals, invoice_reconciliation, purchase_progress, purchase_fulfillments, purchase_acceptances, purchase_submissions, purchase_offers;

CREATE OR REPLACE FUNCTION record_premium_period_change() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE operation premium_period_changes;
BEGIN
 IF TG_OP='UPDATE' AND (OLD.since,OLD.until) IS NOT DISTINCT FROM (NEW.since,NEW.until) THEN RETURN NEW; END IF;
 INSERT INTO premium_period_changes(user_id,premium_id,agreement_id,transaction_id,transaction_started_at,old_period,new_period,ledger_entries)
 VALUES(NEW.user_id,NEW.id,(SELECT agreement_id FROM premium_subscriptions WHERE user_id=NEW.user_id),pg_current_xact_id(),transaction_timestamp(),
 CASE WHEN TG_OP='UPDATE' THEN to_jsonb(OLD) ELSE NULL END,to_jsonb(NEW),
 COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.created_at,t.id) FROM transactions t JOIN premium_transaction_observation o ON o.ledger_id=t.id WHERE t.user_id=NEW.user_id AND o.transaction_id=pg_current_xact_id()),'[]'::jsonb)) RETURNING * INTO operation;
 INSERT INTO contract_premium_operations(declaration_id,operation_id,evidence)
 SELECT d.id,operation.id,to_jsonb(operation) FROM contract_declarations d
 WHERE d.user_id=NEW.user_id AND (d.processed_at IS NULL OR EXISTS(SELECT 1 FROM contract_cancellation_schedule s WHERE s.declaration_id=d.id AND s.completed_at IS NULL));
 -- A later manual purchase preserves access and the actual debit, but must not
 -- silently change the interpretation of an earlier declaration.
 UPDATE contract_declarations d SET processing_note=concat_ws(E'\n',d.processing_note,'Prüfung erforderlich: bezahlte Erweiterung nach Kündigungseingang; ursprüngliche Grenze, Kauf/Abbuchung und Kommunikation zeitnah rechtlich/finanziell abgleichen. Zugang erhalten; keine automatische Erstattung.')
 WHERE d.user_id=NEW.user_id AND d.effective_end IS NOT NULL AND NEW.until>d.effective_end
 AND NOT EXISTS(SELECT 1 FROM contract_processing_actions a WHERE a.declaration_id=d.id AND a.action IN ('record_external_resolution','legacy_unknown'))
 AND coalesce(d.processing_note,'') NOT LIKE '%bezahlte Erweiterung nach Kündigungseingang%';
 RETURN NEW;
END $$;
