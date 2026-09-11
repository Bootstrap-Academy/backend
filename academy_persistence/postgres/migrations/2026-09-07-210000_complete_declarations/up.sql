-- Original declarations and their exact sent bytes are evidence, independently of an account.
CREATE TABLE contract_receipt_access (
    declaration_id uuid PRIMARY KEY REFERENCES contract_declarations(id) ON DELETE CASCADE,
    secret_hash text NOT NULL
);
CREATE TABLE contract_delivery (
    declaration_id uuid NOT NULL REFERENCES contract_declarations(id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('receipt', 'resolution', 'internal')),
    recipient text NOT NULL,
    requested_agreement_id uuid, -- submitted identifier, including an unmatched reference; no FK
    subject text NOT NULL,
    body text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    attempts bigint NOT NULL DEFAULT 0,
    next_attempt_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    accepted_at timestamptz,
    last_error text,
    PRIMARY KEY (declaration_id, kind)
);
CREATE INDEX contract_delivery_pending ON contract_delivery(next_attempt_at) WHERE accepted_at IS NULL;
CREATE TABLE contract_cancellation_schedule (
    declaration_id uuid PRIMARY KEY REFERENCES contract_declarations(id) ON DELETE CASCADE,
    agreement_id uuid NOT NULL REFERENCES premium_renewal_agreements(id),
    requested_end timestamptz NOT NULL,
    established_recipient text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    completed_at timestamptz,
    effective_end timestamptz
);
CREATE INDEX contract_cancellation_pending ON contract_cancellation_schedule(agreement_id) WHERE completed_at IS NULL;
CREATE FUNCTION protect_contract_evidence() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_TABLE_NAME = 'contract_declarations' THEN
    IF (to_jsonb(NEW)-ARRAY['user_id','processed_at','effective_end','processing_note']) IS DISTINCT FROM
       (to_jsonb(OLD)-ARRAY['user_id','processed_at','effective_end','processing_note']) THEN
      RAISE EXCEPTION 'Original declaration is immutable';
    END IF;
  ELSIF TG_TABLE_NAME = 'contract_delivery' THEN
    IF (NEW.declaration_id,NEW.kind,NEW.recipient,NEW.subject,NEW.body,NEW.requested_agreement_id,NEW.created_at)
       IS DISTINCT FROM (OLD.declaration_id,OLD.kind,OLD.recipient,OLD.subject,OLD.body,OLD.requested_agreement_id,OLD.created_at) THEN
      RAISE EXCEPTION 'Confirmation bytes are immutable';
    END IF;
  ELSIF TG_TABLE_NAME = 'contract_cancellation_schedule' THEN
    IF (NEW.declaration_id,NEW.agreement_id,NEW.requested_end,NEW.established_recipient,NEW.created_at)
       IS DISTINCT FROM (OLD.declaration_id,OLD.agreement_id,OLD.requested_end,OLD.established_recipient,OLD.created_at) THEN
      RAISE EXCEPTION 'Original cancellation schedule is immutable';
    END IF;
  ELSE
    RAISE EXCEPTION 'Receipt access and observations are immutable';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER immutable_declaration BEFORE UPDATE ON contract_declarations FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TRIGGER immutable_contract_delivery BEFORE UPDATE ON contract_delivery FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TRIGGER immutable_contract_access BEFORE UPDATE ON contract_receipt_access FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
-- Existing rows have no reliable delivery evidence. Do not fabricate accepted SMTP status or resend legacy text.
CREATE TABLE contract_account_observation (
 declaration_id uuid PRIMARY KEY REFERENCES contract_declarations(id) ON DELETE CASCADE,
 user_id uuid NOT NULL,
 agreement_id uuid,
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE contract_period_observation (
 declaration_id uuid NOT NULL REFERENCES contract_declarations(id) ON DELETE CASCADE,
 premium_id uuid NOT NULL,
 since timestamptz NOT NULL,
 until timestamptz NOT NULL,
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 source text NOT NULL,
 PRIMARY KEY(declaration_id,premium_id,since,until,source)
);
CREATE FUNCTION observe_declared_premium_period() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' THEN
   INSERT INTO contract_period_observation(declaration_id,premium_id,since,until,source)
   SELECT d.id,OLD.id,OLD.since,OLD.until,'before_write' FROM contract_declarations d
   WHERE d.user_id=OLD.user_id AND (d.processed_at IS NULL OR EXISTS(SELECT 1 FROM contract_cancellation_schedule s WHERE s.declaration_id=d.id AND s.completed_at IS NULL))
   ON CONFLICT DO NOTHING;
 END IF;
 INSERT INTO contract_period_observation(declaration_id,premium_id,since,until,source)
 SELECT d.id,NEW.id,NEW.since,NEW.until,'after_write' FROM contract_declarations d
 WHERE d.user_id=NEW.user_id AND (d.processed_at IS NULL OR EXISTS(SELECT 1 FROM contract_cancellation_schedule s WHERE s.declaration_id=d.id AND s.completed_at IS NULL))
 ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER observe_declared_premium AFTER INSERT OR UPDATE ON premium FOR EACH ROW EXECUTE FUNCTION observe_declared_premium_period();

CREATE TRIGGER immutable_contract_schedule BEFORE UPDATE ON contract_cancellation_schedule FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TRIGGER immutable_contract_account_observation BEFORE UPDATE ON contract_account_observation FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TRIGGER immutable_contract_period_observation BEFORE UPDATE ON contract_period_observation FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
