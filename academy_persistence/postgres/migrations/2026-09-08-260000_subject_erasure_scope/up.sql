-- Original paid-contract intent is independent of erasure of an account or
-- later fresh service subject. Old missing declaration evidence stays unknown.
ALTER TABLE commercial_erasure_intake ADD COLUMN declaration jsonb NOT NULL
 DEFAULT '{"paid_contract_intent":"not_recorded","source":"existing_receipt_without_scope"}';
CREATE FUNCTION commercial_record_erasure_intake(p_subject uuid,p_received timestamptz) RETURNS void LANGUAGE sql AS $$
 INSERT INTO commercial_erasure_intake(subject,received_at,declaration)
 SELECT id,p_received,jsonb_build_object('version','data_erasure_action_v1',
  'scope',CASE WHEN commercial_is_learning_subject(id) THEN 'learning_data' ELSE 'ordinary_account' END,
  'paid_contract_intent','no_paid_cancellation_in_this_action',
  'declaration','Erase this account or limited learning data/access; existing paid rights and separate actual contract declarations remain.')
 FROM users WHERE id=p_subject ON CONFLICT(subject) DO NOTHING;
$$;

CREATE OR REPLACE FUNCTION commercial_lock_subject(p_subject uuid,p_evidenced_absence boolean DEFAULT false) RETURNS uuid LANGUAGE plpgsql AS $$
DECLARE result uuid; account users;
BEGIN
 SELECT * INTO account FROM users WHERE id=p_subject FOR UPDATE;
 SELECT case_id INTO result FROM commercial_learning_subjects WHERE subject=p_subject;
 IF FOUND THEN
  PERFORM 1 FROM commercial_cases WHERE id=result FOR UPDATE;
  RETURN result;
 END IF;
 IF account.id IS NOT NULL THEN
  INSERT INTO commercial_cases(subject,contact,contact_verified,contact_provenance)
  VALUES(p_subject,account.email,account.email_verified,jsonb_build_object('kind','live_account','observed_at',clock_timestamp(),'verified',account.email_verified))
  ON CONFLICT(subject) DO NOTHING;
 ELSIF p_evidenced_absence THEN
  INSERT INTO commercial_cases(subject,review_reason,inventory)
  VALUES(p_subject,'Original Events financial evidence; historic account/erasure facts require review',
   '{"backend":"unknown","events":"preserved","skills":"unknown","challenges":"unknown"}'::jsonb)
  ON CONFLICT(subject) DO NOTHING;
 END IF;
 SELECT id INTO result FROM commercial_cases WHERE subject=p_subject FOR UPDATE;
 IF result IS NULL THEN RAISE EXCEPTION 'No evidenced commercial recipient'; END IF;
 RETURN result;
END $$;

CREATE OR REPLACE FUNCTION commercial_erasure_envelope() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE c uuid; request_id uuid:=gen_random_uuid(); receipt timestamptz;
 wallet coins; captured bigint; provenance text; snapshot jsonb; limited boolean; intent jsonb; erased timestamptz:=clock_timestamp();
BEGIN
 -- The existing moderation self-erasure guard remains authoritative. This trigger
 -- supplies preservation even to an authorized database path, not deletion authority.
 IF current_setting('academy.moderation_erasure_subject',true) IS DISTINCT FROM OLD.id::text THEN
  RAISE EXCEPTION 'Authenticated self-erasure context required';
 END IF;
 c:=commercial_lock_subject(OLD.id);
 limited:=commercial_is_learning_subject(OLD.id);
 SELECT * INTO wallet FROM coins WHERE user_id=OLD.id FOR UPDATE;
 SELECT commercial_captured_units(c) INTO captured;
 SELECT i.id,i.received_at,i.declaration INTO request_id,receipt,intent FROM commercial_erasure_intake i WHERE i.subject=OLD.id;
 IF NOT FOUND THEN request_id:=gen_random_uuid(); END IF;
 provenance:=CASE WHEN receipt IS NULL THEN 'database_erasure_observed' ELSE 'authenticated_service_receipt' END;
 INSERT INTO commercial_requests(id,case_id,kind,source,received_at,evidence)
 VALUES(request_id,c,CASE WHEN limited THEN 'learning_data_erasure' ELSE 'account_erasure' END,provenance,receipt,
  jsonb_build_object('observed_deletion_at',erased,'subject',OLD.id,'scope',CASE WHEN limited THEN 'learning_data' ELSE 'ordinary_account' END,
   'declaration',coalesce(intent,'{"paid_contract_intent":"not_recorded"}'::jsonb),'earlier_declaration','not_inferred'))
 ON CONFLICT DO NOTHING;
 INSERT INTO commercial_subject_erasures(subject,case_id,request_id,erased_at,scope)
 VALUES(OLD.id,c,request_id,erased,CASE WHEN limited THEN 'learning_data' ELSE 'ordinary_account' END) ON CONFLICT DO NOTHING;
 IF limited THEN
  UPDATE commercial_learning_subjects SET erased_at=erased,authority_epoch=authority_epoch+1 WHERE subject=OLD.id AND erased_at IS NULL;
  UPDATE commercial_learning_keys SET revoked_at=erased WHERE subject=OLD.id AND revoked_at IS NULL;
 END IF;
 UPDATE commercial_cases SET erased_at=CASE WHEN limited THEN erased_at ELSE erased END,closed_at=NULL,review_due_at=least(review_due_at,erased),
  inventory=inventory||'{"backend":"preserved","events":"pending","skills":"pending","challenges":"pending"}'::jsonb
 WHERE id=c;
 snapshot:=jsonb_build_object('available',coalesce(wallet.coins,0),'withheld',coalesce(wallet.withheld_coins,0),
  'captured_purchase_units',captured,'prior_cash_refunds','unknown','policy','fungible_reward_first',
  'unused_purchase_upper_bound',least(coalesce(wallet.coins,0),captured),'not_a_final_liability_total',true);
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence) VALUES(c,'wallet_boundary',OLD.id::text,snapshot) ON CONFLICT DO NOTHING;
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 SELECT c,'legacy_purchase',id,to_jsonb(o) FROM paypal_coin_orders o WHERE user_id=OLD.id ON CONFLICT DO NOTHING;
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 SELECT c,'ledger',id::text,to_jsonb(t) FROM transactions t WHERE user_id=OLD.id ON CONFLICT DO NOTHING;
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 SELECT c,'premium_operation',id::text,to_jsonb(p) FROM premium_period_changes p WHERE user_id=OLD.id
 AND NOT EXISTS(SELECT 1 FROM contract_premium_operations d WHERE d.operation_id=p.id) ON CONFLICT DO NOTHING;
 INSERT INTO commercial_obligations(id,case_id,source,source_key,component,units,cash_units,status,original)
 VALUES(gen_random_uuid(),c,'backend',OLD.id::text,'wallet_available',coalesce(wallet.coins,0),CASE WHEN captured=0 THEN 0 ELSE NULL END,
  'established',snapshot) ON CONFLICT DO NOTHING;
 INSERT INTO commercial_obligations(id,case_id,source,source_key,component,units,status,original)
 VALUES(gen_random_uuid(),c,'backend',OLD.id::text,'wallet_withheld',coalesce(wallet.withheld_coins,0),'pending_evidence',snapshot) ON CONFLICT DO NOTHING;
 -- Preserve actual live period/balance observations before cascade. These are
 -- prospective erasure facts, not reconstructed historic purchase/payment lots.
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 SELECT c,'premium_right',p.id::text,to_jsonb(p)||jsonb_build_object('observed_at',erased,'subject',OLD.id,'request_id',request_id,
  'scope','Existing paid period at erasure; original dates remain authoritative, no renewal or new term inferred')
 FROM premium p WHERE p.user_id=OLD.id ON CONFLICT DO NOTHING;
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 SELECT c,'heart_balance_at_erasure',OLD.id::text,to_jsonb(h)||jsonb_build_object('observed_at',erased,'request_id',request_id,
  'scope','Actual balance and last refill only; no unconsumed purchased-lot allocation inferred')
 FROM hearts h WHERE h.user_id=OLD.id ON CONFLICT DO NOTHING;
 PERFORM commercial_preserve_records(c,OLD.id);
 RETURN OLD;
END $$;

-- Canonical lookup uses the actual erased subject, not its claimant alias. A
-- new T2 erasure can never return the old T0 request for a later T1 contract.
CREATE FUNCTION commercial_subject_erasure(p_subject uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('protocol',1,'case_id',e.case_id,'subject',e.subject,'erased_at',e.erased_at,
  'scope',e.scope,'request',to_jsonb(r)) FROM commercial_subject_erasures e
 JOIN commercial_requests r ON r.id=e.request_id WHERE e.subject=p_subject;
$$;
-- Backfill only proved old erasures and their original requests, not scope.
INSERT INTO commercial_subject_erasures(subject,case_id,request_id,erased_at,scope)
 SELECT c.subject,c.id,r.id,c.erased_at,'ordinary_account' FROM commercial_cases c JOIN commercial_requests r
 ON r.case_id=c.id AND r.kind='account_erasure' WHERE c.erased_at IS NOT NULL ON CONFLICT DO NOTHING;

CREATE FUNCTION commercial_owned_service_subject(p_claimant uuid,p_subject uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT p_claimant=p_subject OR EXISTS(SELECT 1 FROM commercial_learning_subjects l JOIN commercial_cases c ON c.id=l.case_id
 WHERE l.subject=p_subject AND c.subject=p_claimant);
$$;

CREATE OR REPLACE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c uuid; target uuid;
BEGIN
 IF p_operation='erasure' THEN RETURN commercial_subject_erasure(p_actor); END IF;
 IF p_operation='learning_erasure_target' THEN
  SELECT id INTO c FROM commercial_cases WHERE subject=p_actor;
  target:=(p_body->>'subject')::uuid;
  IF c IS NULL OR NOT commercial_personal_proof(c,p_body) OR p_body->'erase_learning_data' IS DISTINCT FROM 'true'::jsonb
   OR p_body-'_claim_hash'-'_moderation_hash'-'subject'-'erase_learning_data'<>'{}'::jsonb
   OR NOT EXISTS(SELECT 1 FROM commercial_learning_subjects WHERE subject=target AND case_id=c)
   THEN RAISE EXCEPTION 'Personal claimant declaration for this exact limited subject required'; END IF;
  RETURN jsonb_build_object('subject',target,'already_erased',EXISTS(SELECT 1 FROM commercial_subject_erasures WHERE subject=target),
   'paid_cancellation_requested_by_this_action',false);
 END IF;
 IF p_operation='owned_service_subject' THEN
  target:=(p_body->>'subject')::uuid;
  IF NOT coalesce(commercial_owned_service_subject(p_actor,target),false) THEN RETURN NULL; END IF;
  RETURN to_jsonb(target);
 END IF;
 IF p_operation IN ('split_cash','record_cash_payment','outcome') THEN RETURN commercial_partial_cash_operation(p_operation,p_actor,p_body); END IF;
 IF p_operation='request_wallet_cash' THEN RETURN commercial_wallet_cash_request(p_actor,p_body); END IF;
 IF p_operation='return_wallet_cash' THEN RETURN commercial_wallet_cash_return(p_actor,p_body); END IF;
 IF p_operation IN ('restore_credit','cash_basis_review') THEN RETURN commercial_value_operation(p_operation,p_actor,p_body); END IF;
 IF p_operation='purchase_authorize' THEN RETURN commercial_purchase_authorize(p_actor,p_body); END IF;
 IF p_operation IN ('learning_start','learning_access','learning_summary','learning_revoke','learning_authority') THEN RETURN commercial_learning_operation(p_operation,p_actor,p_body); END IF;
 RETURN commercial_claim_operation(p_operation,p_actor,p_body);
END $$;

ALTER FUNCTION commercial_case_export(uuid) RENAME TO commercial_case_export_before_subject_scope;
CREATE FUNCTION commercial_case_export(p_subject uuid) RETURNS jsonb LANGUAGE sql AS $$
 SELECT commercial_case_export_before_subject_scope(p_subject)||jsonb_build_object(
  'subject_erasures',coalesce((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.erased_at,e.subject)
   FROM commercial_subject_erasures e JOIN commercial_cases c ON c.id=e.case_id WHERE c.subject=p_subject),'[]'),
  'service_erasure_intakes',coalesce((SELECT jsonb_agg(to_jsonb(i) ORDER BY i.received_at,i.subject)
   FROM commercial_erasure_intake i JOIN commercial_learning_subjects l ON l.subject=i.subject JOIN commercial_cases c ON c.id=l.case_id
   WHERE c.subject=p_subject),'[]'));
$$;
