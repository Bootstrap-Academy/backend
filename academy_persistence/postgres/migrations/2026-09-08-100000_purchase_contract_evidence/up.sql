-- New prospective evidence only. Never infer historical offers from current assets.
CREATE TABLE purchase_offers (
 id uuid PRIMARY KEY,
 user_id uuid NOT NULL,
 source text NOT NULL CHECK(source IN ('backend','skills','events','paypal')),
 offer jsonb NOT NULL,
 terms_pdf bytea NOT NULL,
 withdrawal_pdf bytea NOT NULL,
 created_at timestamptz NOT NULL,
 expires_at timestamptz NOT NULL
);
CREATE INDEX purchase_offers_user ON purchase_offers(user_id,created_at);
CREATE TRIGGER immutable_purchase_offer BEFORE UPDATE ON purchase_offers FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE purchase_submissions (
 order_id uuid PRIMARY KEY REFERENCES purchase_offers(id),
 request jsonb NOT NULL,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable_purchase_submission BEFORE UPDATE ON purchase_submissions FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE purchase_acceptances (
 order_id uuid PRIMARY KEY REFERENCES purchase_offers(id),
 acceptance_sequence bigint GENERATED ALWAYS AS IDENTITY UNIQUE,
 accepted_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 confirmation_body text NOT NULL,
 message_metadata jsonb NOT NULL
);
CREATE TRIGGER immutable_purchase_acceptance BEFORE UPDATE ON purchase_acceptances FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE purchase_fulfillments (
 order_id uuid PRIMARY KEY REFERENCES purchase_offers(id),
 fulfilled_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 result jsonb NOT NULL,
 statement text NOT NULL
);
CREATE TRIGGER immutable_purchase_fulfillment BEFORE UPDATE ON purchase_fulfillments FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE purchase_progress (
 order_id uuid PRIMARY KEY REFERENCES purchase_offers(id),
 state text NOT NULL DEFAULT 'offered' CHECK(state IN ('offered','accepted','awaiting_payment','paid','fulfilled','failed','review')),
 review_reason text,
 smtp_accepted_at timestamptz,
 attempts bigint NOT NULL DEFAULT 0,
 generation bigint NOT NULL DEFAULT 0,
 lease_until timestamptz,
 next_attempt_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX purchase_delivery_due ON purchase_progress(next_attempt_at) WHERE smtp_accepted_at IS NULL;
-- Evidence is intentionally independent of account deletion. Access and category-
-- specific retention/holds are required; no historical adulthood/contact is inferred.
CREATE TABLE invoice_originals (
 invoice_number text PRIMARY KEY,
 pdf bytea NOT NULL,
 provenance text NOT NULL,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable_invoice_original BEFORE UPDATE ON invoice_originals FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE invoice_reconciliation (
 invoice_number text PRIMARY KEY,
 state text NOT NULL CHECK(state IN ('evidence_missing','evidenced')),
 reason text NOT NULL,
 evidence jsonb,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE TABLE purchase_debits (
 order_id uuid PRIMARY KEY REFERENCES purchase_offers(id),
 ledger jsonb NOT NULL,
 tender_observations jsonb NOT NULL
);
CREATE TRIGGER immutable_purchase_debit BEFORE UPDATE ON purchase_debits FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE purchase_delivery_attempts (
 order_id uuid NOT NULL REFERENCES purchase_offers(id),
 generation bigint NOT NULL,
 observation text NOT NULL CHECK(observation IN ('started','smtp_accepted','definite_rejection','handoff_uncertain')),
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(order_id,generation,observation)
);
CREATE TRIGGER immutable_purchase_delivery_attempt BEFORE UPDATE ON purchase_delivery_attempts FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE contract_purchase_observations (
 declaration_id uuid NOT NULL REFERENCES contract_declarations(id) ON DELETE CASCADE,
 order_id uuid NOT NULL,
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 evidence jsonb NOT NULL,
 PRIMARY KEY(declaration_id,order_id)
);
CREATE TRIGGER immutable_contract_purchase_observation BEFORE UPDATE ON contract_purchase_observations FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE FUNCTION observe_contract_purchases(declaration uuid, account uuid) RETURNS void LANGUAGE sql AS $$
 INSERT INTO contract_purchase_observations(declaration_id,order_id,evidence)
 SELECT declaration,o.id,jsonb_build_object('offer',o.offer,'acceptance',to_jsonb(a),'debit',to_jsonb(d),'no_charge',(o.offer->'product'->>'coins')::bigint=0,'fulfillment',(SELECT to_jsonb(f) FROM purchase_fulfillments f WHERE f.order_id=o.id),'ordering','commit_order_unknown_at_receipt')
 FROM purchase_offers o JOIN purchase_acceptances a ON a.order_id=o.id LEFT JOIN purchase_debits d ON d.order_id=o.id
 WHERE o.user_id=account AND (d.order_id IS NOT NULL OR ((o.offer->'product'->>'coins')::bigint=0 AND EXISTS(SELECT 1 FROM purchase_progress p WHERE p.order_id=o.id AND p.state IN ('paid','fulfilled','review')))) AND o.offer->'product'->>'kind' LIKE 'premium_%'
 ON CONFLICT DO NOTHING;
$$;
CREATE FUNCTION observe_new_purchase_debit() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE declaration uuid;
BEGIN
 FOR declaration IN SELECT id FROM contract_declarations WHERE user_id=(SELECT user_id FROM purchase_offers WHERE id=NEW.order_id) LOOP
  PERFORM observe_contract_purchases(declaration,(SELECT user_id FROM purchase_offers WHERE id=NEW.order_id));
 END LOOP;
 RETURN NEW;
END $$;
CREATE TRIGGER observe_new_purchase_debit AFTER INSERT ON purchase_debits FOR EACH ROW EXECUTE FUNCTION observe_new_purchase_debit();
ALTER TABLE premium_period_changes ADD COLUMN purchase_evidence jsonb;
CREATE OR REPLACE FUNCTION record_premium_period_change() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE operation premium_period_changes;
BEGIN
 IF TG_OP='UPDATE' AND (OLD.since,OLD.until) IS NOT DISTINCT FROM (NEW.since,NEW.until) THEN RETURN NEW; END IF;
 INSERT INTO premium_period_changes(user_id,premium_id,agreement_id,transaction_id,transaction_started_at,old_period,new_period,ledger_entries,purchase_evidence)
 VALUES(NEW.user_id,NEW.id,(SELECT agreement_id FROM premium_subscriptions WHERE user_id=NEW.user_id),pg_current_xact_id(),transaction_timestamp(),
 CASE WHEN TG_OP='UPDATE' THEN to_jsonb(OLD) ELSE NULL END,to_jsonb(NEW),
 COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.created_at,t.id) FROM transactions t JOIN premium_transaction_observation o ON o.ledger_id=t.id WHERE t.user_id=NEW.user_id AND o.transaction_id=pg_current_xact_id()),'[]'::jsonb) || COALESCE((SELECT jsonb_build_array(ledger) FROM purchase_debits WHERE order_id=nullif(current_setting('academy.purchase_order_id',true),'')::uuid),'[]'::jsonb),
 (SELECT jsonb_build_object('order_id',o.id,'offer',o.offer,'accepted_at',a.accepted_at,'original_ledger',d.ledger,'tender_observations',d.tender_observations) FROM purchase_offers o JOIN purchase_acceptances a ON a.order_id=o.id LEFT JOIN purchase_debits d ON d.order_id=o.id WHERE o.id=nullif(current_setting('academy.purchase_order_id',true),'')::uuid)) RETURNING * INTO operation;
 INSERT INTO contract_premium_operations(declaration_id,operation_id,evidence)
 SELECT d.id,operation.id,to_jsonb(operation) FROM contract_declarations d
 WHERE d.user_id=NEW.user_id AND (EXISTS(SELECT 1 FROM contract_purchase_observations x WHERE x.declaration_id=d.id AND x.order_id=nullif(current_setting('academy.purchase_order_id',true),'')::uuid) OR d.processed_at IS NULL OR EXISTS(SELECT 1 FROM contract_cancellation_schedule s WHERE s.declaration_id=d.id AND s.completed_at IS NULL));
 -- A later manual purchase preserves access and the actual debit, but must not
 -- silently change the interpretation of an earlier declaration.
 UPDATE contract_declarations d SET processing_note=concat_ws(E'\n',d.processing_note,'Prüfung erforderlich: bezahlte Erweiterung nach Kündigungseingang; ursprüngliche Grenze, Kauf/Abbuchung und Kommunikation zeitnah rechtlich/finanziell abgleichen. Zugang erhalten; keine automatische Erstattung.')
 WHERE d.user_id=NEW.user_id AND d.effective_end IS NOT NULL AND NEW.until>d.effective_end
 AND NOT EXISTS(SELECT 1 FROM contract_processing_actions a WHERE a.declaration_id=d.id AND a.action IN ('record_external_resolution','legacy_unknown'))
 AND coalesce(d.processing_note,'') NOT LIKE '%bezahlte Erweiterung nach Kündigungseingang%';
 RETURN NEW;
END $$;

CREATE TABLE purchase_cash_captures(order_id uuid PRIMARY KEY REFERENCES purchase_offers(id), evidence jsonb NOT NULL);
CREATE TRIGGER immutable_purchase_cash_capture BEFORE UPDATE ON purchase_cash_captures FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE paypal_receipt_artifacts(order_id text PRIMARY KEY,artifact jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE TRIGGER immutable_paypal_receipt_artifact BEFORE UPDATE ON paypal_receipt_artifacts FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE paypal_receipt_observations(order_id text NOT NULL,attempt uuid NOT NULL,observation text NOT NULL,observed_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(order_id,attempt,observation));
CREATE TRIGGER immutable_paypal_receipt_observation BEFORE UPDATE ON paypal_receipt_observations FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE paypal_contract_orders(contract_order_id uuid PRIMARY KEY REFERENCES purchase_offers(id),paypal_order_id text NOT NULL UNIQUE);
