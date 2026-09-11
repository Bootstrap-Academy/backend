-- Purchase/refund capacity belongs to the claimant, not a disposable technical
-- subject. These aggregate facts do not assign fungible coins to historical lots.
CREATE TABLE commercial_cash_basis (
 case_id uuid PRIMARY KEY REFERENCES commercial_cases(id),
 prior_refund_units bigint NOT NULL CHECK(prior_refund_units>=0),
 assessment jsonb NOT NULL, reviewed_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
ALTER TABLE commercial_reservations ADD COLUMN purchase_capacity_units bigint CHECK(purchase_capacity_units>=0 AND purchase_capacity_units<=units);
UPDATE commercial_reservations r SET purchase_capacity_units=r.units FROM commercial_obligations o
 WHERE o.id=r.obligation_id AND r.mode='cash' AND o.component IN ('wallet_available','wallet_withheld','active_wallet_cash');
CREATE FUNCTION commercial_captured_units(p_case uuid) RETURNS bigint LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN EXISTS(SELECT 1 FROM users WHERE id=c.subject)
 THEN coalesce((SELECT sum(o.coins) FROM paypal_coin_orders o WHERE o.user_id=c.subject AND o.captured_at IS NOT NULL),0)
 ELSE (SELECT (e.evidence->>'captured_purchase_units')::bigint FROM commercial_evidence e WHERE e.case_id=c.id AND e.category='wallet_boundary' AND e.source_key=c.subject::text)
 END FROM commercial_cases c WHERE c.id=p_case;
$$;
CREATE FUNCTION commercial_cash_capacity(p_case uuid) RETURNS bigint LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN b.case_id IS NULL OR commercial_captured_units(p_case) IS NULL
  OR EXISTS(SELECT 1 FROM commercial_reservations r JOIN commercial_obligations o ON o.id=r.obligation_id
   WHERE o.case_id=p_case AND r.mode='cash' AND r.state IN ('reserved','uncertain','completed') AND r.purchase_capacity_units IS NULL)
 THEN NULL ELSE greatest(0,commercial_captured_units(p_case)-b.prior_refund_units-coalesce((
  SELECT sum(r.purchase_capacity_units) FROM commercial_reservations r JOIN commercial_obligations o ON o.id=r.obligation_id
  WHERE o.case_id=p_case AND r.mode='cash' AND r.state IN ('reserved','uncertain','completed')),0)) END
 FROM (SELECT 1) sentinel LEFT JOIN commercial_cash_basis b ON b.case_id=p_case;
$$;
CREATE FUNCTION commercial_cash_reservation_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE o commercial_obligations; cap bigint;
BEGIN
 IF NEW.mode<>'cash' THEN RETURN NEW; END IF;
 SELECT * INTO o FROM commercial_obligations WHERE id=NEW.obligation_id;
 PERFORM 1 FROM commercial_cases WHERE id=o.case_id FOR UPDATE;
 IF o.component IN ('wallet_available','wallet_withheld','active_wallet_cash') THEN NEW.purchase_capacity_units:=NEW.units;
 ELSE
  -- A service-price remedy can be independent of unused-wallet redemption.
  -- Its supported effect on purchased capacity must be explicit, never guessed
  -- from the fact that money was paid or that the claimant earned rewards.
  NEW.purchase_capacity_units:=(NEW.request->>'purchase_capacity_units')::bigint;
  IF NEW.purchase_capacity_units IS NULL OR length(coalesce(NEW.request->>'capacity_basis',''))<30 THEN
   RAISE EXCEPTION 'Assess this service remedy against the claimant purchase/refund basis';
  END IF;
 END IF;
 cap:=commercial_cash_capacity(o.case_id);
 IF NEW.purchase_capacity_units>0 AND (cap IS NULL OR NEW.purchase_capacity_units>cap) THEN
  RAISE EXCEPTION 'Known unreserved claimant purchase/refund capacity required; uncertainty remains held';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_cash_reservation_guard BEFORE INSERT ON commercial_reservations FOR EACH ROW EXECUTE FUNCTION commercial_cash_reservation_guard();
CREATE FUNCTION commercial_reservation_capacity_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.purchase_capacity_units IS DISTINCT FROM OLD.purchase_capacity_units THEN RAISE EXCEPTION 'Original reservation capacity is immutable'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_reservation_capacity_immutable BEFORE UPDATE ON commercial_reservations FOR EACH ROW EXECUTE FUNCTION commercial_reservation_capacity_immutable();

CREATE FUNCTION commercial_value_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c uuid; root_subject uuid; service_id uuid; o commercial_obligations; prior commercial_journal;
 command uuid:=(p_body->>'command_id')::uuid; payload jsonb:=p_body-'_claim_hash'-'_moderation_hash'-'_staff_session'-'_staff_refresh_hash';
 amount bigint; prior_units bigint; result jsonb; ledger uuid; cash_before bigint;
BEGIN
 IF command IS NULL THEN RAISE EXCEPTION 'Exact value disposition request identity required'; END IF;
 IF p_operation='cash_basis_review' THEN
  c:=(p_body->>'case_id')::uuid;
  IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Current administrator MFA required'; END IF;
  SELECT subject INTO root_subject FROM commercial_cases WHERE id=c;
 ELSE
  root_subject:=p_actor; SELECT id INTO c FROM commercial_cases WHERE subject=p_actor;
  IF c IS NULL OR NOT commercial_personal_proof(c,p_body) THEN RAISE EXCEPTION 'Current personal claimant proof required'; END IF;
 END IF;
 -- The command fence precedes all owning user/case/wallet locks, matching the
 -- common commercial dispatcher. A waiter returns the immutable first result
 -- instead of reapplying changed balance or initial-assessment prerequisites.
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF p_operation='cash_basis_review' THEN
  IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Staff authority changed while waiting for original command'; END IF;
 ELSIF NOT commercial_personal_proof(c,p_body) THEN
  RAISE EXCEPTION 'Claimant proof changed while waiting for original command';
 END IF;
 SELECT * INTO prior FROM commercial_journal WHERE command_id=command;
 IF FOUND THEN
  IF prior.case_id IS DISTINCT FROM c OR prior.actor IS DISTINCT FROM p_actor OR prior.kind<>p_operation OR prior.request<>payload THEN RAISE EXCEPTION 'Conflicting value disposition replay'; END IF;
  RETURN prior.result;
 END IF;
 IF p_operation='restore_credit' THEN
  SELECT subject INTO service_id FROM commercial_learning_subjects WHERE case_id=c AND erased_at IS NULL;
  IF service_id IS NULL OR NOT commercial_purchase_lock(service_id) THEN RAISE EXCEPTION 'Current explicitly elected limited service subject required'; END IF;
 ELSE
  PERFORM commercial_lock_subject(root_subject);
 END IF;
 IF p_operation='cash_basis_review' THEN
  IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Staff authority changed while waiting'; END IF;
  prior_units:=(p_body->>'prior_refund_units')::bigint;
  IF prior_units IS NULL OR prior_units<0 OR length(coalesce(p_body->>'assessment',''))<40
   OR p_body->'legacy_refund_records_checked' IS DISTINCT FROM 'true'::jsonb
   OR jsonb_typeof(p_body->'evidence') IS DISTINCT FROM 'object' OR p_body->'evidence'='{}'::jsonb
   OR commercial_captured_units(c) IS NULL THEN RAISE EXCEPTION 'Actual captured purchase facts and separately checked historic refund records required'; END IF;
  IF EXISTS(SELECT 1 FROM commercial_cash_basis WHERE case_id=c) THEN
   IF p_body->'corrects_previous_assessment' IS DISTINCT FROM 'true'::jsonb THEN RAISE EXCEPTION 'Explicit supported correction to prior assessment required'; END IF;
  END IF;
  INSERT INTO commercial_cash_basis(case_id,prior_refund_units,assessment) VALUES(c,prior_units,payload||jsonb_build_object('actor',p_actor,'scope','Historic refunds before the commercial reservation journal; journal outcomes excluded to prevent double deduction'))
  ON CONFLICT(case_id) DO UPDATE SET prior_refund_units=excluded.prior_refund_units,assessment=excluded.assessment,reviewed_at=clock_timestamp();
  result:=jsonb_build_object('cash_capacity',commercial_cash_capacity(c),'captured_purchase_units',commercial_captured_units(c),'assessment_recorded',true,'paid',false);
 ELSIF p_operation='restore_credit' THEN
  IF NOT commercial_personal_proof(c,p_body) OR p_body->'choose_coins' IS DISTINCT FROM 'true'::jsonb THEN RAISE EXCEPTION 'Current explicit coin-return election required'; END IF;
  SELECT * INTO o FROM commercial_obligations WHERE id=(p_body->>'obligation_id')::uuid AND case_id=c FOR UPDATE;
  amount:=(p_body->>'units')::bigint;
  IF o.id IS NULL OR o.status<>'established' OR amount IS NULL OR amount<=0 OR amount>commercial_remaining(o.id) THEN RAISE EXCEPTION 'Established unreserved remaining credit required'; END IF;
  ledger:=gen_random_uuid(); cash_before:=commercial_cash_capacity(c);
  INSERT INTO commercial_reservations(id,obligation_id,units,mode,state,request,outcome,completed_at)
  VALUES(command,o.id,amount,'wallet','completed',payload,jsonb_build_object('subject',service_id,'ledger_id',ledger,'kind','existing_credit_return','purchased_lots','not_created'),clock_timestamp());
  INSERT INTO coins(user_id,coins,withheld_coins) VALUES(service_id,amount,0)
   ON CONFLICT(user_id) DO UPDATE SET coins=coins.coins+excluded.coins;
  INSERT INTO transactions(id,user_id,created_at,coins,description,include_in_credit_note)
   VALUES(ledger,service_id,clock_timestamp(),amount,'Return of retained MorphCoin entitlement',false);
  result:=jsonb_build_object('reservation_id',command,'subject',service_id,'ledger_id',ledger,'units_returned',amount,
   'wallet_balance',(SELECT coins FROM coins WHERE user_id=service_id),'claimant_cash_capacity',cash_before,'value_expires',false,'cash_paid',false);
 ELSE RAISE EXCEPTION 'Unsupported value operation'; END IF;
 INSERT INTO commercial_journal(case_id,obligation_id,actor,command_id,kind,request,result)
 VALUES(c,o.id,p_actor,command,p_operation,payload,result);
 RETURN result;
END $$;
-- Preserve actual new ledger facts independently of a later fresh-subject
-- erasure. Reward provenance does not reset/deplete aggregate purchased value.
CREATE FUNCTION commercial_learning_ledger_evidence() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 SELECT l.case_id,'learning_ledger',NEW.id::text,to_jsonb(NEW)||jsonb_build_object('scope','actual limited-service ledger observation; no captured cash or historical coin lot inferred')
 FROM commercial_learning_subjects l WHERE l.subject=NEW.user_id ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_learning_ledger_evidence AFTER INSERT ON transactions FOR EACH ROW EXECUTE FUNCTION commercial_learning_ledger_evidence();
CREATE OR REPLACE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation IN ('restore_credit','cash_basis_review') THEN RETURN commercial_value_operation(p_operation,p_actor,p_body); END IF;
 IF p_operation='purchase_authorize' THEN RETURN commercial_purchase_authorize(p_actor,p_body); END IF;
 IF p_operation IN ('learning_start','learning_access','learning_summary','learning_revoke','learning_authority') THEN RETURN commercial_learning_operation(p_operation,p_actor,p_body); END IF;
 RETURN commercial_claim_operation(p_operation,p_actor,p_body);
END $$;
