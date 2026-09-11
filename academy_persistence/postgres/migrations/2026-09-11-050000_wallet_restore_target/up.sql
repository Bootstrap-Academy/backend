-- Bind new retained-credit returns to the selected existing subject; preserve old receipts.
CREATE OR REPLACE FUNCTION commercial_value_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c uuid; root_subject uuid; service_id uuid; o commercial_obligations; prior commercial_journal;
 command uuid; payload jsonb;
 amount bigint; prior_units bigint; result jsonb; ledger uuid; cash_before bigint;
BEGIN
 -- Restore's post-wait reads are supported only under fresh statement snapshots,
 -- including historical replay. Keep cash's command then payload evaluation order.
 IF p_operation='restore_credit' AND current_setting('transaction_isolation')<>'read committed' THEN
  RAISE EXCEPTION 'Wallet restoration requires READ COMMITTED';
 END IF;
 command:=(p_body->>'command_id')::uuid;
 payload:=p_body-'_claim_hash'-'_moderation_hash'-'_staff_session'-'_staff_refresh_hash';
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
  -- Completed legacy commands above keep their exact original body and target.
  IF jsonb_typeof(p_body->'expected_subject') IS DISTINCT FROM 'string' THEN
   RAISE EXCEPTION 'Exact selected learning subject required';
  END IF;
  SELECT subject INTO service_id FROM commercial_learning_subjects
   WHERE case_id=c AND subject=(p_body->>'expected_subject')::uuid AND erased_at IS NULL;
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
  -- Owning waits must not turn a prepared S1 return into an S2 return or use
  -- stale personal authority. The existing amount/effect expressions stay intact.
  IF NOT commercial_personal_proof(c,p_body)
   OR NOT EXISTS(SELECT 1 FROM commercial_learning_subjects WHERE case_id=c AND subject=service_id AND erased_at IS NULL)
   OR NOT commercial_learning_allowed(service_id) THEN
   RAISE EXCEPTION 'Current claimant proof and exact selected learning admission required after waiting';
  END IF;
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
