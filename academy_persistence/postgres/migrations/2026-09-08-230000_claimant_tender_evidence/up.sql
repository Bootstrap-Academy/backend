-- Prospectively capture claimant-level facts with the actual new purchase ledger
-- entry, then attach that frozen observation before debit-copy observers run.
-- A later record insertion never samples current facts as historic funding.
-- Owning purchase acceptance already holds service user -> claimant case ->
-- order/wallet locks. This evidence reader acquires no additional lock.
CREATE FUNCTION commercial_tender_funding(p_subject uuid) RETURNS jsonb
LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object(
  'version','claimant_fungible_tender_v1',
  'case_id',c.id,'claimant',c.subject,'service_subject',l.subject,
  'observed_at',statement_timestamp(),
  'scope','Claimant aggregate facts observed in the owning purchase wallet transaction at ledger insertion; no historical coin lots or service-refund allocation inferred',
  'captured_purchase_units',commercial_captured_units(c.id),
  'captured_basis_kind',CASE WHEN EXISTS(SELECT 1 FROM users u WHERE u.id=c.subject)
   THEN 'live_original_claimant_capture_records' ELSE 'preserved_original_claimant_erasure_observation' END,
  'captured_basis',CASE WHEN EXISTS(SELECT 1 FROM users u WHERE u.id=c.subject) THEN
   jsonb_build_object('records',coalesce((SELECT jsonb_agg(jsonb_build_object('order_id',p.id,'coins',p.coins,'captured_at',p.captured_at,'invoice_number',p.invoice_number) ORDER BY p.id)
    FROM paypal_coin_orders p WHERE p.user_id=c.subject AND p.captured_at IS NOT NULL),'[]'::jsonb))
   ELSE jsonb_build_object(
    'wallet_observation',(SELECT e.evidence FROM commercial_evidence e WHERE e.case_id=c.id AND e.category='wallet_boundary' AND e.source_key=c.subject::text),
    'preserved_capture_records',coalesce((SELECT jsonb_agg(jsonb_build_object('source_key',e.source_key,'order_id',e.evidence->'id','coins',e.evidence->'coins','captured_at',e.evidence->'captured_at','invoice_number',e.evidence->'invoice_number') ORDER BY e.source_key)
     FROM commercial_evidence e WHERE e.case_id=c.id AND e.category='legacy_purchase' AND e.evidence->>'captured_at' IS NOT NULL),'[]'::jsonb),
    'record_completeness','No missing historic per-order record inferred from an aggregate observation') END,
  'historic_prior_refund_status',CASE WHEN b.case_id IS NULL THEN 'unknown' ELSE 'operator_reviewed' END,
  'historic_prior_refund_units',b.prior_refund_units,
  'historic_prior_refund_review',CASE WHEN b.case_id IS NULL THEN NULL ELSE to_jsonb(b) END,
  'cash_capacity_reservations',coalesce((SELECT jsonb_agg(jsonb_build_object(
   'reservation_id',r.id,'obligation_id',r.obligation_id,'component',o.component,
   'units',r.units,'purchase_capacity_units',r.purchase_capacity_units,'state',r.state,
   'external_reference',r.external_reference,'recorded_outcome',r.outcome,
   'outcome_scope','Retained operator-recorded outcome; this snapshot performs no external payment verification') ORDER BY r.id)
   FROM commercial_reservations r JOIN commercial_obligations o ON o.id=r.obligation_id
   WHERE o.case_id=c.id AND r.mode='cash' AND r.state IN ('reserved','uncertain','completed')),'[]'::jsonb),
  'known_pending_purchase_capacity_units',coalesce((SELECT sum(r.purchase_capacity_units)
   FROM commercial_reservations r JOIN commercial_obligations o ON o.id=r.obligation_id
   WHERE o.case_id=c.id AND r.mode='cash' AND r.state IN ('reserved','uncertain')),0),
  'known_recorded_completed_purchase_capacity_units',coalesce((SELECT sum(r.purchase_capacity_units)
   FROM commercial_reservations r JOIN commercial_obligations o ON o.id=r.obligation_id
   WHERE o.case_id=c.id AND r.mode='cash' AND r.state='completed'),0),
  'unknown_reservation_capacity_count',(SELECT count(*)
   FROM commercial_reservations r JOIN commercial_obligations o ON o.id=r.obligation_id
   WHERE o.case_id=c.id AND r.mode='cash' AND r.state IN ('reserved','uncertain','completed') AND r.purchase_capacity_units IS NULL),
  'remaining_purchase_capacity',commercial_cash_capacity(c.id),
  'capacity_scope','Aggregate purchased-unit ceiling after reviewed historic refunds and held/completed journal capacity; distinct from wallet balance and independently established service remedies',
  'coin_policy','fungible_reward_first_100_coins_per_euro',
  'purchased_reward_allocation','unknown',
  'cash_payment_performed',false)
 FROM commercial_learning_subjects l JOIN commercial_cases c ON c.id=l.case_id
 LEFT JOIN commercial_cash_basis b ON b.case_id=c.id WHERE l.subject=p_subject;
$$;

CREATE FUNCTION commercial_capture_purchase_tender() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE funding jsonb; wallet jsonb;
BEGIN
 IF NEW.coins>=0 OR NOT EXISTS(SELECT 1 FROM purchase_offers o WHERE o.id=NEW.id AND o.user_id=NEW.user_id
  AND (o.offer->'product'->>'coins')::numeric=-NEW.coins::numeric) THEN RETURN NEW; END IF;
 -- Only the owning new acceptance path marks its wallet/ledger sequence. A
 -- later record-only insertion cannot turn current balances into old funding.
 IF nullif(current_setting('academy.new_purchase_wallet_ledger',true),'') IS DISTINCT FROM NEW.id::text THEN RETURN NEW; END IF;
 PERFORM set_config('academy.new_purchase_wallet_ledger','',true);
 funding:=commercial_tender_funding(NEW.user_id);
 IF funding IS NULL THEN RETURN NEW; END IF;
 SELECT jsonb_build_object('available_before',c.coins-NEW.coins,'available_after',c.coins,
  'withheld_before',c.withheld_coins,'withheld_after',c.withheld_coins,
  'captured_purchase_units',coalesce((SELECT sum(p.coins) FROM paypal_coin_orders p WHERE p.user_id=NEW.user_id AND p.captured_at IS NOT NULL),0),
  'captured_purchase_basis',coalesce((SELECT jsonb_agg(jsonb_build_object('order_id',p.id,'coins',p.coins,'captured_at',p.captured_at,'invoice_number',p.invoice_number) ORDER BY p.id)
   FROM paypal_coin_orders p WHERE p.user_id=NEW.user_id AND p.captured_at IS NOT NULL),'[]'::jsonb),
  'prior_cash_refunds','unknown','purchased_reward_allocation','unknown') INTO wallet
 FROM coins c WHERE c.user_id=NEW.user_id;
 IF wallet IS NULL THEN RAISE EXCEPTION 'Original purchase wallet observation unavailable'; END IF;
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 VALUES((funding->>'case_id')::uuid,'purchase_tender',NEW.id::text,jsonb_build_object(
  'version','claimant_fungible_tender_v1','original_ledger',to_jsonb(NEW),
  'timing_basis','owning_purchase_wallet_transaction_ledger_insert',
  'observed_at',clock_timestamp(),'technical_subject_observations',wallet,'claimant_funding',funding));
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_capture_purchase_tender AFTER INSERT ON transactions
 FOR EACH ROW EXECUTE FUNCTION commercial_capture_purchase_tender();

CREATE FUNCTION commercial_record_tender_funding() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE subject_id uuid; original jsonb; funding jsonb; wallet jsonb;
BEGIN
 SELECT user_id INTO subject_id FROM purchase_offers WHERE id=NEW.order_id;
 IF NOT commercial_is_learning_subject(subject_id) THEN RETURN NEW; END IF;
 SELECT e.evidence INTO original FROM commercial_evidence e JOIN commercial_learning_subjects l ON l.case_id=e.case_id
 WHERE l.subject=subject_id AND e.category='purchase_tender' AND e.source_key=NEW.order_id::text;
 IF original IS NULL THEN
  NEW.tender_observations:=jsonb_build_object('version','claimant_fungible_tender_v1',
   'funding_observation_status','unavailable_at_original_debit',
   'record_insert_observations',NEW.tender_observations,
   'record_insert_observation_scope','Later local query; not evidence of original claimant funding or original wallet boundary',
   'claimant_funding',NULL,'captured_purchase_units',NULL,'captured_purchase_basis',NULL,
   'available_before',NULL,'available_after',NULL,'withheld_before',NULL,'withheld_after',NULL,
   'prior_cash_refunds','unknown','purchased_reward_allocation','unknown');
  RETURN NEW;
 END IF;
 IF original->'original_ledger' IS DISTINCT FROM NEW.ledger THEN RAISE EXCEPTION 'Preserved purchase tender ledger differs from debit original'; END IF;
 funding:=original->'claimant_funding'; wallet:=original->'technical_subject_observations';
 -- Keep raw local observations, but explicitly separate them from the actual
 -- claimant basis. A fresh technical subject's local zero is not claimant P=0.
 NEW.tender_observations:=jsonb_build_object(
  'version','claimant_fungible_tender_v1',
  'funding_observation_status','preserved_at_original_ledger_insertion',
  'timing_basis',original->'timing_basis','observed_at',original->'observed_at',
  'record_insert_observations',NEW.tender_observations,
  'record_insert_observation_scope','Local query at record insertion; original wallet and claimant facts use the separately preserved ledger observation',
  'technical_subject_observations',wallet,
  'claimant_funding',funding,
  'available_before',wallet->'available_before',
  'available_after',wallet->'available_after',
  'withheld_before',wallet->'withheld_before',
  'withheld_after',wallet->'withheld_after',
  'captured_purchase_units',funding->'captured_purchase_units',
  'captured_purchase_basis',funding->'captured_basis',
  'prior_cash_refunds',jsonb_build_object('status',funding->'historic_prior_refund_status',
   'historic_units',funding->'historic_prior_refund_units',
   'recorded_completed_capacity_units',funding->'known_recorded_completed_purchase_capacity_units'),
  'purchased_reward_allocation','unknown');
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_record_tender_funding BEFORE INSERT ON purchase_debits
 FOR EACH ROW EXECUTE FUNCTION commercial_record_tender_funding();
