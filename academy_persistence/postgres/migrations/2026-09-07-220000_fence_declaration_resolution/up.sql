-- Record every actual period mutation, including writers that precede the receipt lock.
-- No historical mutation/commit timestamps are fabricated by this migration.
-- A ledger insert can run inside a savepoint (its xmin is then a child XID).
-- Capture its actual top-level transaction identity at insertion, without
-- guessing from descriptions/timestamps of preexisting ledger rows.
CREATE TABLE premium_transaction_observation (
 ledger_id uuid PRIMARY KEY REFERENCES transactions(id) ON DELETE CASCADE,
 transaction_id xid8 NOT NULL
);
CREATE INDEX premium_transaction_observation_txn ON premium_transaction_observation(transaction_id);
CREATE TRIGGER immutable_premium_transaction_observation BEFORE UPDATE ON premium_transaction_observation FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE FUNCTION observe_premium_transaction() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO premium_transaction_observation VALUES(NEW.id,pg_current_xact_id());
 RETURN NEW;
END $$;
CREATE TRIGGER observe_premium_transaction AFTER INSERT ON transactions FOR EACH ROW WHEN (NEW.description='Premium') EXECUTE FUNCTION observe_premium_transaction();
CREATE TABLE premium_period_changes (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 premium_id uuid NOT NULL,
 agreement_id uuid,
 transaction_id xid8 NOT NULL,
 transaction_started_at timestamptz NOT NULL,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 old_period jsonb,
 new_period jsonb NOT NULL,
 ledger_entries jsonb NOT NULL
);
CREATE INDEX premium_period_changes_user_time ON premium_period_changes(user_id,recorded_at);
CREATE TABLE contract_premium_operations (
 declaration_id uuid NOT NULL REFERENCES contract_declarations(id) ON DELETE CASCADE,
 operation_id bigint NOT NULL,
 evidence jsonb NOT NULL,
 PRIMARY KEY(declaration_id,operation_id)
);
CREATE TRIGGER immutable_premium_period_change BEFORE UPDATE ON premium_period_changes FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TRIGGER immutable_contract_premium_operation BEFORE UPDATE ON contract_premium_operations FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE FUNCTION record_premium_period_change() RETURNS trigger LANGUAGE plpgsql AS $$
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
CREATE TRIGGER record_premium_period AFTER INSERT OR UPDATE ON premium FOR EACH ROW EXECUTE FUNCTION record_premium_period_change();

CREATE TABLE contract_processing_actions (
 declaration_id uuid PRIMARY KEY REFERENCES contract_declarations(id) ON DELETE CASCADE,
 action text NOT NULL CHECK(action IN ('record_external_resolution','schedule_premium_cancellation','legacy_unknown')),
 recorded_at timestamptz NOT NULL,
 effective_end timestamptz,
 note text,
 previous_effective_end timestamptz,
 previous_resolution jsonb,
 previous_schedule jsonb
);
ALTER TABLE contract_delivery ADD COLUMN superseded_at timestamptz;
ALTER TABLE contract_delivery DROP CONSTRAINT contract_delivery_kind_check;
ALTER TABLE contract_delivery ADD CONSTRAINT contract_delivery_kind_check CHECK(kind IN ('receipt','resolution','internal','external_resolution'));
-- Earlier releases did not retain the selected action. Do not guess which
-- completed records authorize future automatic determinations or old retries.
INSERT INTO contract_processing_actions(declaration_id,action,recorded_at,effective_end,note,previous_effective_end,previous_resolution,previous_schedule)
SELECT id,'legacy_unknown',processed_at,effective_end,processing_note,effective_end,
 (SELECT to_jsonb(m) FROM contract_delivery m WHERE m.declaration_id=d.id AND m.kind='resolution'),(SELECT to_jsonb(s) FROM contract_cancellation_schedule s WHERE s.declaration_id=d.id)
FROM contract_declarations d WHERE processed_at IS NOT NULL;
UPDATE contract_delivery m SET superseded_at=clock_timestamp(),last_error='LegacyActionRequiresReview'
WHERE m.kind='resolution' AND m.accepted_at IS NULL AND EXISTS(SELECT 1 FROM contract_processing_actions a WHERE a.declaration_id=m.declaration_id);
CREATE TRIGGER immutable_contract_processing_action BEFORE UPDATE ON contract_processing_actions FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
