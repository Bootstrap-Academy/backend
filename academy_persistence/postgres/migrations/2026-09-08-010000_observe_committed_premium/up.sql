-- A later snapshot does not prove a purchase committed before an earlier receipt.
-- Record only an actual upper bound observed from a different transaction.
-- Existing operations deliberately have no retrospective commitment witness.
CREATE TABLE premium_period_commit_observation (
 operation_id bigint PRIMARY KEY REFERENCES premium_period_changes(id) ON DELETE CASCADE,
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE FUNCTION observe_committed_premium_period() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE original_transaction xid8;
BEGIN
 SELECT transaction_id INTO STRICT original_transaction FROM premium_period_changes WHERE id=NEW.operation_id;
 IF original_transaction=pg_current_xact_id() OR NOT pg_visible_in_snapshot(original_transaction,pg_current_snapshot()) THEN
   RAISE EXCEPTION 'A committed period must be observed from a different transaction';
 END IF;
 -- Ignore caller-provided dates: only this database observation is evidence.
 NEW.observed_at=clock_timestamp();
 RETURN NEW;
END $$;
CREATE TRIGGER observe_committed_premium_period BEFORE INSERT ON premium_period_commit_observation FOR EACH ROW EXECUTE FUNCTION observe_committed_premium_period();
CREATE TRIGGER immutable_premium_period_commit_observation BEFORE UPDATE ON premium_period_commit_observation FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
