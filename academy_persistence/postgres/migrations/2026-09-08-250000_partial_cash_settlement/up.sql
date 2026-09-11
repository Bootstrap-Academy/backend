-- A split partitions an existing hold. It neither qualifies a new cash claim
-- nor erases an unknown historical effect on the aggregate purchase ceiling.
ALTER TABLE commercial_reservations DROP CONSTRAINT commercial_reservations_state_check;
ALTER TABLE commercial_reservations ADD CHECK(state IN ('reserved','uncertain','completed','failed','split'));
CREATE TABLE commercial_reservation_splits (
 parent_id uuid PRIMARY KEY REFERENCES commercial_reservations(id),
 case_id uuid NOT NULL REFERENCES commercial_cases(id), command_id uuid NOT NULL UNIQUE,
 original jsonb NOT NULL, children jsonb NOT NULL, assessment jsonb NOT NULL,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable_commercial_split BEFORE UPDATE ON commercial_reservation_splits
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
ALTER TABLE commercial_reservations ADD COLUMN parent_id uuid REFERENCES commercial_reservation_splits(parent_id);

CREATE TABLE commercial_cash_payments (
 id uuid PRIMARY KEY, case_id uuid NOT NULL REFERENCES commercial_cases(id),
 external_reference text NOT NULL UNIQUE CHECK(length(trim(external_reference))>=3),
 units bigint NOT NULL CHECK(units>0), currency text NOT NULL CHECK(currency='EUR'),
 payment_method text NOT NULL CHECK(length(trim(payment_method))>0),
 destination text NOT NULL CHECK(length(trim(destination))>0),
 occurred_at timestamptz NOT NULL, evidence jsonb NOT NULL, actor uuid NOT NULL,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable_commercial_payment BEFORE UPDATE ON commercial_cash_payments
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE commercial_cash_allocations (
 reservation_id uuid PRIMARY KEY REFERENCES commercial_reservations(id),
 payment_id uuid NOT NULL REFERENCES commercial_cash_payments(id),
 units bigint NOT NULL CHECK(units>0), authority jsonb NOT NULL,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable_commercial_allocation BEFORE UPDATE ON commercial_cash_allocations
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();

CREATE OR REPLACE FUNCTION commercial_cash_reservation_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE o commercial_obligations; cap bigint; split commercial_reservation_splits; parent commercial_reservations; child jsonb;
BEGIN
 IF NEW.mode<>'cash' THEN
  IF NEW.parent_id IS NOT NULL THEN RAISE EXCEPTION 'Only existing cash holds can be partitioned'; END IF;
  RETURN NEW;
 END IF;
 SELECT * INTO o FROM commercial_obligations WHERE id=NEW.obligation_id;
 PERFORM 1 FROM commercial_cases WHERE id=o.case_id FOR UPDATE;
 IF NEW.parent_id IS NOT NULL THEN
  SELECT * INTO split FROM commercial_reservation_splits WHERE parent_id=NEW.parent_id AND case_id=o.case_id;
  SELECT * INTO parent FROM commercial_reservations WHERE id=NEW.parent_id;
  SELECT value INTO child FROM jsonb_array_elements(split.children) WHERE value->>'id'=NEW.id::text;
  IF split.parent_id IS NULL OR parent.state<>'split' OR parent.obligation_id<>NEW.obligation_id
   OR child IS NULL OR NEW.units IS DISTINCT FROM (child->>'units')::bigint
   OR NEW.purchase_capacity_units IS DISTINCT FROM (child->>'purchase_capacity_units')::bigint
   OR NEW.state IS DISTINCT FROM split.original->>'state'
   OR NEW.request IS DISTINCT FROM jsonb_build_object('split_parent',NEW.parent_id,'split_command',split.command_id,'original_request',split.original->'request') THEN
   RAISE EXCEPTION 'Exact immutable parent allocation required';
  END IF;
  -- Deferred conservation checks require every child and its exact original
  -- capacity, including NULL. Current lower/unknown capacity cannot rewrite it.
  RETURN NEW;
 END IF;
 IF o.component IN ('wallet_available','wallet_withheld','active_wallet_cash') THEN NEW.purchase_capacity_units:=NEW.units;
 ELSE
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

CREATE FUNCTION commercial_split_conservation() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE s commercial_reservation_splits; r commercial_reservations; total numeric; caps numeric; unknowns bigint; count_children bigint;
BEGIN
 SELECT * INTO s FROM commercial_reservation_splits WHERE parent_id=NEW.parent_id;
 SELECT * INTO r FROM commercial_reservations WHERE id=s.parent_id;
 SELECT sum(units),sum(purchase_capacity_units),count(*) FILTER(WHERE purchase_capacity_units IS NULL),count(*)
 INTO total,caps,unknowns,count_children FROM commercial_reservations WHERE parent_id=s.parent_id;
 IF r.state<>'split' OR total IS DISTINCT FROM (s.original->>'units')::numeric
  OR count_children<>jsonb_array_length(s.children)
  OR (s.original->>'purchase_capacity_units' IS NULL AND unknowns<>count_children)
  OR (s.original->>'purchase_capacity_units' IS NOT NULL AND (unknowns<>0 OR caps IS DISTINCT FROM (s.original->>'purchase_capacity_units')::numeric)) THEN
  RAISE EXCEPTION 'Partition must preserve all original units and known or unknown capacity';
 END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER commercial_split_conservation AFTER INSERT ON commercial_reservation_splits
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION commercial_split_conservation();

CREATE FUNCTION commercial_reservation_partition_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.id<>OLD.id OR NEW.obligation_id<>OLD.obligation_id OR NEW.units<>OLD.units OR NEW.mode<>OLD.mode
  OR NEW.parent_id IS DISTINCT FROM OLD.parent_id OR NEW.request<>OLD.request THEN
  RAISE EXCEPTION 'Original reservation identity and amount are immutable';
 END IF;
 IF OLD.state IN ('completed','failed','split') AND NEW IS DISTINCT FROM OLD THEN
  RAISE EXCEPTION 'Terminal reservation or partition parent cannot be revived';
 END IF;
 IF NEW.state='split' AND NOT EXISTS(SELECT 1 FROM commercial_reservation_splits s WHERE s.parent_id=OLD.id AND s.original=to_jsonb(OLD)) THEN
  RAISE EXCEPTION 'Original parent and complete partition plan required';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_reservation_partition_guard BEFORE UPDATE ON commercial_reservations
 FOR EACH ROW EXECUTE FUNCTION commercial_reservation_partition_guard();

-- Both the old scalar writer and new multi-allocation writer use one namespace.
-- The fence is after the owning case lock; it never acquires another case.
CREATE FUNCTION commercial_external_payment_reference_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.external_reference IS NULL THEN RETURN NEW; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-payment:'||NEW.external_reference,0));
 IF TG_TABLE_NAME='commercial_cash_payments' THEN
  IF EXISTS(SELECT 1 FROM commercial_reservations WHERE external_reference=NEW.external_reference) THEN
   RAISE EXCEPTION 'External payment already recorded by scalar settlement';
  END IF;
 ELSE
  IF EXISTS(SELECT 1 FROM commercial_cash_payments WHERE external_reference=NEW.external_reference) THEN
   RAISE EXCEPTION 'External payment already allocated; use its exact original receipt';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_payment_reference_guard BEFORE INSERT ON commercial_cash_payments
 FOR EACH ROW EXECUTE FUNCTION commercial_external_payment_reference_guard();
CREATE TRIGGER commercial_reservation_reference_guard BEFORE INSERT OR UPDATE OF external_reference ON commercial_reservations
 FOR EACH ROW EXECUTE FUNCTION commercial_external_payment_reference_guard();

CREATE FUNCTION commercial_payment_authority(p_obligation uuid,p_authority jsonb,p_method text,p_destination text) RETURNS boolean LANGUAGE plpgsql STABLE AS $$
DECLARE o commercial_obligations; rule text:=p_authority->>'rule';
BEGIN
 SELECT * INTO o FROM commercial_obligations WHERE id=p_obligation;
 IF o.id IS NULL OR jsonb_typeof(p_authority) IS DISTINCT FROM 'object'
  OR length(trim(coalesce(p_method,'')))=0 OR length(trim(coalesce(p_destination,'')))=0
  OR p_authority->>'obligation_id' IS DISTINCT FROM o.id::text
  OR p_authority->>'payment_method' IS DISTINCT FROM p_method OR p_authority->>'destination' IS DISTINCT FROM p_destination
  OR length(coalesce(p_authority->>'cash_election_evidence',''))<20
  OR length(coalesce(p_authority->>'claimant_recipient_authority',''))<20
  OR length(coalesce(p_authority->>'applicable_terms_evidence',''))<20 THEN RETURN false; END IF;
 IF o.component IN ('wallet_available','wallet_withheld','active_wallet_cash') THEN
  IF rule='wallet_original_payment' THEN
   RETURN p_method='paypal' AND p_authority->>'original_payer_destination'=p_destination
    AND length(coalesce(p_authority->>'original_payment_reference',''))>=3
    AND length(coalesce(p_authority->>'original_payer_evidence',''))>=20;
  ELSIF rule='wallet_agreed_alternative' THEN
   RETURN p_authority->'cost_free'='true'::jsonb AND length(coalesce(p_authority->>'express_alternative_agreement',''))>=20;
  END IF;
  RETURN false;
 END IF;
 -- A service-price cash election has its own agreed recipient/method. A
 -- top-up merchant/payer identifier is not inferred to be that destination.
 RETURN rule='service_price_cash' AND length(coalesce(p_authority->>'service_cash_basis',''))>=20
  AND length(coalesce(p_authority->>'agreed_payment_method_evidence',''))>=20
  AND (p_method='paypal' OR (p_authority->'cost_free'='true'::jsonb
   AND length(coalesce(p_authority->>'express_alternative_agreement',''))>=20));
END $$;

CREATE FUNCTION commercial_partial_cash_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c uuid:=(p_body->>'case_id')::uuid; command uuid:=(p_body->>'command_id')::uuid;
 payload jsonb:=p_body-'_claim_hash'-'_moderation_hash'-'_staff_session'-'_staff_refresh_hash';
 prior commercial_journal; r commercial_reservations; owner_id uuid; child jsonb; allocation jsonb;
 result jsonb; total numeric:=0; caps numeric:=0; unknowns integer:=0; child_count integer:=0;
 target text; payment uuid; payment_units bigint; method text; destination text; occurred timestamptz;
BEGIN
 IF command IS NULL OR c IS NULL OR p_actor IS NULL OR NOT commercial_staff_active(p_actor,p_body)
  OR length(trim(coalesce(p_body->>'assessment','')))<20 THEN RAISE EXCEPTION 'Exact staff request, current MFA and specific assessment required'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Staff authority changed while waiting'; END IF;
 SELECT * INTO prior FROM commercial_journal WHERE command_id=command;
 IF FOUND THEN
  IF prior.case_id IS DISTINCT FROM c OR prior.actor IS DISTINCT FROM p_actor OR prior.kind<>p_operation OR prior.request<>payload THEN RAISE EXCEPTION 'Conflicting commercial command replay'; END IF;
  RETURN prior.result;
 END IF;
 SELECT subject INTO owner_id FROM commercial_cases WHERE id=c;
 IF owner_id IS NULL OR commercial_lock_subject(owner_id) IS DISTINCT FROM c THEN RAISE EXCEPTION 'Exact claimant case required'; END IF;
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Staff authority changed while waiting for case'; END IF;

 IF p_operation='split_cash' THEN
  SELECT * INTO r FROM commercial_reservations WHERE id=(p_body->>'reservation_id')::uuid FOR UPDATE;
  IF r.id IS NULL OR r.mode<>'cash' OR r.state NOT IN ('reserved','uncertain')
   OR NOT EXISTS(SELECT 1 FROM commercial_obligations WHERE id=r.obligation_id AND case_id=c)
   OR jsonb_typeof(p_body->'children') IS DISTINCT FROM 'array'
   OR jsonb_array_length(p_body->'children') NOT BETWEEN 2 AND 100 THEN RAISE EXCEPTION 'Existing active cash leaf and complete bounded partition required'; END IF;
  FOR child IN SELECT value FROM jsonb_array_elements(p_body->'children') LOOP
   IF jsonb_typeof(child) IS DISTINCT FROM 'object' OR NOT child ? 'purchase_capacity_units'
    OR jsonb_typeof(child->'units') IS DISTINCT FROM 'number' OR child->>'units' !~ '^[1-9][0-9]*$'
    OR child->>'id' IS NULL OR (child->>'id')::uuid=r.id
    OR (child->>'purchase_capacity_units' IS NOT NULL AND (jsonb_typeof(child->'purchase_capacity_units')<>'number' OR child->>'purchase_capacity_units' !~ '^[0-9]+$'
     OR (child->>'purchase_capacity_units')::numeric>(child->>'units')::numeric)) THEN RAISE EXCEPTION 'Exact positive child units and explicit inherited capacity required'; END IF;
   total:=total+(child->>'units')::numeric; child_count:=child_count+1;
   IF child->>'purchase_capacity_units' IS NULL THEN unknowns:=unknowns+1; ELSE caps:=caps+(child->>'purchase_capacity_units')::numeric; END IF;
  END LOOP;
  IF total<>r.units OR (r.purchase_capacity_units IS NULL AND unknowns<>child_count)
   OR (r.purchase_capacity_units IS NOT NULL AND (unknowns<>0 OR caps<>r.purchase_capacity_units)) THEN RAISE EXCEPTION 'Original units and unknown or known purchased capacity must be conserved'; END IF;
  INSERT INTO commercial_reservation_splits(parent_id,case_id,command_id,original,children,assessment)
   VALUES(r.id,c,command,to_jsonb(r),p_body->'children',payload);
  UPDATE commercial_reservations SET state='split' WHERE id=r.id;
  FOR child IN SELECT value FROM jsonb_array_elements(p_body->'children') LOOP
   INSERT INTO commercial_reservations(id,obligation_id,units,mode,state,request,purchase_capacity_units,parent_id)
   VALUES((child->>'id')::uuid,r.obligation_id,(child->>'units')::bigint,'cash',r.state,
    jsonb_build_object('split_parent',r.id,'split_command',command,'original_request',r.request),
    (child->>'purchase_capacity_units')::bigint,r.id);
  END LOOP;
  result:=jsonb_build_object('reservation_id',r.id,'state','split','children',p_body->'children','inherited_state',r.state,
   'units_held',r.units,'purchase_capacity_units',r.purchase_capacity_units,'cash_paid',false,'new_value_issued',false);
 ELSIF p_operation='record_cash_payment' THEN
  payment:=(p_body->>'payment_id')::uuid; payment_units:=(p_body->>'paid_units')::bigint;
  method:=p_body->>'payment_method'; destination:=p_body->>'destination'; occurred:=(p_body->>'occurred_at')::timestamptz;
  IF payment IS NULL OR payment_units IS NULL OR payment_units<=0 OR p_body->>'currency' IS DISTINCT FROM 'EUR'
   OR occurred IS NULL OR occurred>clock_timestamp() OR length(trim(coalesce(method,'')))=0 OR length(trim(coalesce(destination,'')))=0
   OR length(trim(coalesce(p_body->>'external_reference','')))<3
   OR jsonb_typeof(p_body->'evidence') IS DISTINCT FROM 'object' OR p_body->'evidence'='{}'::jsonb
   OR jsonb_typeof(p_body->'allocations') IS DISTINCT FROM 'array' OR jsonb_array_length(p_body->'allocations') NOT BETWEEN 1 AND 100 THEN
   RAISE EXCEPTION 'Exact occurred external payment, evidence and complete allocations required';
  END IF;
  FOR allocation IN SELECT value FROM jsonb_array_elements(p_body->'allocations') LOOP
   SELECT * INTO r FROM commercial_reservations WHERE id=(allocation->>'reservation_id')::uuid FOR UPDATE;
   IF r.id IS NULL OR r.mode<>'cash' OR r.state NOT IN ('reserved','uncertain')
    OR NOT EXISTS(SELECT 1 FROM commercial_obligations WHERE id=r.obligation_id AND case_id=c)
    OR (allocation->>'units')::bigint IS DISTINCT FROM r.units
    OR NOT coalesce(commercial_payment_authority(r.obligation_id,allocation->'authority',method,destination),false) THEN
    RAISE EXCEPTION 'Each exact active leaf needs its own applicable cash election, method and recipient evidence';
   END IF;
   total:=total+r.units;
  END LOOP;
  IF total<>payment_units THEN RAISE EXCEPTION 'All allocated units must equal the actual payment'; END IF;
  INSERT INTO commercial_cash_payments(id,case_id,external_reference,units,currency,payment_method,destination,occurred_at,evidence,actor)
   VALUES(payment,c,p_body->>'external_reference',payment_units,'EUR',method,destination,occurred,
    jsonb_build_object('kind','operator_recorded_external_outcome','assessment',payload,'provider_verified_by_system',false,'payment_execution_performed',false),p_actor);
  FOR allocation IN SELECT value FROM jsonb_array_elements(p_body->'allocations') LOOP
   INSERT INTO commercial_cash_allocations(reservation_id,payment_id,units,authority)
    VALUES((allocation->>'reservation_id')::uuid,payment,(allocation->>'units')::bigint,allocation->'authority');
   UPDATE commercial_reservations SET state='completed',completed_at=clock_timestamp(),
    outcome=jsonb_build_object('kind','operator_recorded_external_outcome','payment_id',payment,'units',(allocation->>'units')::bigint)
    WHERE id=(allocation->>'reservation_id')::uuid;
  END LOOP;
  result:=jsonb_build_object('payment_id',payment,'recorded_paid_units',payment_units,'allocations',p_body->'allocations',
   'evidence_kind','operator_recorded_external_outcome','provider_verified_by_system',false,'payment_execution_performed',false);
 ELSIF p_operation='outcome' THEN
  -- Preserve the scalar contract and its original command receipts. Completed
  -- split parents are never valid targets; partial payments use exact leaves.
  SELECT x.* INTO r FROM commercial_reservations x JOIN commercial_obligations o ON o.id=x.obligation_id
   WHERE x.id=(p_body->>'reservation_id')::uuid AND o.case_id=c FOR UPDATE OF x;
  target:=p_body->>'state';
  IF r.id IS NULL OR r.mode<>'cash' OR r.state NOT IN ('reserved','uncertain') OR target NOT IN ('completed','failed','uncertain')
   OR target IS NULL THEN RAISE EXCEPTION 'Existing active cash leaf and valid outcome required'; END IF;
  IF jsonb_typeof(p_body->'evidence') IS DISTINCT FROM 'object' OR p_body->'evidence'='{}'::jsonb THEN RAISE EXCEPTION 'Actual outcome evidence required'; END IF;
  IF target='failed' AND p_body->'evidence'->'definitive_no_payment' IS DISTINCT FROM 'true'::jsonb THEN RAISE EXCEPTION 'Uncertainty cannot release reserved funds'; END IF;
  IF target='completed' AND (length(coalesce(p_body->>'external_reference',''))<3 OR (p_body->>'paid_units')::bigint IS DISTINCT FROM r.units
   OR p_body->'evidence'->'destination_matches_reservation' IS DISTINCT FROM 'true'::jsonb
   OR NOT coalesce(commercial_payment_authority(r.obligation_id,p_body->'payment_authority',p_body->>'payment_method',p_body->>'destination'),false)) THEN
   RAISE EXCEPTION 'Exact payment and applicable leaf method, election and recipient evidence required';
  END IF;
  UPDATE commercial_reservations SET state=target,outcome=payload,completed_at=CASE WHEN target='completed' THEN clock_timestamp() END,
   external_reference=CASE WHEN target='completed' THEN p_body->>'external_reference' ELSE external_reference END WHERE id=r.id;
  result:=jsonb_build_object('reservation_id',r.id,'state',target,'evidence_kind','operator_recorded_external_outcome');
 ELSE RAISE EXCEPTION 'Unsupported partial cash operation'; END IF;
 INSERT INTO commercial_journal(case_id,obligation_id,actor,command_id,kind,request,result)
  VALUES(c,CASE WHEN p_operation='record_cash_payment' THEN NULL ELSE r.obligation_id END,p_actor,command,p_operation,payload,result);
 RETURN result;
END $$;

CREATE OR REPLACE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation IN ('split_cash','record_cash_payment','outcome') THEN RETURN commercial_partial_cash_operation(p_operation,p_actor,p_body); END IF;
 IF p_operation='request_wallet_cash' THEN RETURN commercial_wallet_cash_request(p_actor,p_body); END IF;
 IF p_operation='return_wallet_cash' THEN RETURN commercial_wallet_cash_return(p_actor,p_body); END IF;
 IF p_operation IN ('restore_credit','cash_basis_review') THEN RETURN commercial_value_operation(p_operation,p_actor,p_body); END IF;
 IF p_operation='purchase_authorize' THEN RETURN commercial_purchase_authorize(p_actor,p_body); END IF;
 IF p_operation IN ('learning_start','learning_access','learning_summary','learning_revoke','learning_authority') THEN RETURN commercial_learning_operation(p_operation,p_actor,p_body); END IF;
 RETURN commercial_claim_operation(p_operation,p_actor,p_body);
END $$;

ALTER FUNCTION commercial_case_export(uuid) RENAME TO commercial_case_export_before_partial;
CREATE FUNCTION commercial_case_export(p_subject uuid) RETURNS jsonb LANGUAGE sql AS $$
 SELECT commercial_case_export_before_partial(p_subject)||jsonb_build_object(
  'reservation_splits',coalesce((SELECT jsonb_agg(to_jsonb(s) ORDER BY s.recorded_at,s.parent_id) FROM commercial_reservation_splits s JOIN commercial_cases c ON c.id=s.case_id WHERE c.subject=p_subject),'[]'),
  'cash_payments',coalesce((SELECT jsonb_agg(to_jsonb(p) ORDER BY p.recorded_at,p.id) FROM commercial_cash_payments p JOIN commercial_cases c ON c.id=p.case_id WHERE c.subject=p_subject),'[]'),
  'cash_allocations',coalesce((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.recorded_at,a.reservation_id) FROM commercial_cash_allocations a JOIN commercial_cash_payments p ON p.id=a.payment_id JOIN commercial_cases c ON c.id=p.case_id WHERE c.subject=p_subject),'[]'));
$$;
