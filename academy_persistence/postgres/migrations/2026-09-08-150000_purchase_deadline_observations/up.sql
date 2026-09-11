-- Prospective bound is an immutable offer term; old accepted rows have no
-- invented duration. The separate observation is an upper bound on committed
-- provision visibility, never a claimed exact COMMIT timestamp.
CREATE FUNCTION purchase_provision_deadline(id uuid) RETURNS timestamptz LANGUAGE sql STABLE AS $$
 SELECT least((o.offer->'product'->>'service_starts_at')::timestamptz,
   a.accepted_at + (o.offer->>'provision_window_seconds')::bigint * interval '1 second')
 FROM purchase_offers o LEFT JOIN purchase_acceptances a ON a.order_id=o.id WHERE o.id=$1
$$;
CREATE TABLE purchase_provision_observations (
 order_id uuid PRIMARY KEY REFERENCES purchase_offers(id),
 observed_at timestamptz NOT NULL,
 evidence jsonb NOT NULL,
 statement text NOT NULL
);
CREATE TRIGGER immutable_purchase_provision_observation BEFORE UPDATE ON purchase_provision_observations
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
