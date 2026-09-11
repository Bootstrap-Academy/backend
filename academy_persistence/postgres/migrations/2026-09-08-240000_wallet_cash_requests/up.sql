-- A personal request can move already existing spendable wallet value into an
-- assessed claim. This is neither cash payment nor newly issued coin value.
CREATE FUNCTION commercial_wallet_cash_request(p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c commercial_cases; receipt commercial_journal; wallet coins;
 command uuid:=(p_body->>'command_id')::uuid; target uuid:=(p_body->>'wallet_subject')::uuid;
 amount bigint:=(p_body->>'units')::bigint; ledger uuid; obligation uuid;
 payload jsonb:=p_body-'_claim_hash'-'_moderation_hash'; result jsonb;
 captured bigint; capacity bigint; observed_basis jsonb; original jsonb;
BEGIN
 IF command IS NULL OR target IS NULL OR amount IS NULL OR amount<=0
  OR jsonb_typeof(p_body->'units') IS DISTINCT FROM 'number'
  OR p_body->'request_cash_review' IS DISTINCT FROM 'true'::jsonb
  OR payload-'command_id'-'wallet_subject'-'units'-'request_cash_review'<>'{}'::jsonb THEN
  RAISE EXCEPTION 'Exact wallet, positive integer amount and explicit cash-assessment request required';
 END IF;
 SELECT * INTO c FROM commercial_cases WHERE subject=p_actor;
 IF c.id IS NULL OR NOT commercial_personal_proof(c.id,p_body) THEN RAISE EXCEPTION 'Current personal claimant proof required'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF NOT commercial_personal_proof(c.id,p_body) THEN RAISE EXCEPTION 'Claimant proof changed while waiting for original request'; END IF;
 SELECT * INTO receipt FROM commercial_journal WHERE command_id=command;
 IF FOUND THEN
  IF receipt.case_id IS DISTINCT FROM c.id OR receipt.actor IS DISTINCT FROM p_actor
   OR receipt.kind<>'request_wallet_cash' OR receipt.request<>payload THEN RAISE EXCEPTION 'Conflicting wallet cash request'; END IF;
  RETURN receipt.result;
 END IF;
 IF NOT ((target=c.subject AND NOT commercial_is_learning_subject(target)) OR EXISTS(
  SELECT 1 FROM commercial_learning_subjects l WHERE l.subject=target AND l.case_id=c.id AND l.erased_at IS NULL)) THEN
  RAISE EXCEPTION 'Exact wallet of this claimant required';
 END IF;
 -- Financial claims do not mint ordinary or learning authority. Their owning
 -- wallet row precedes the claimant and coin locks, including a limited subject.
 PERFORM 1 FROM users WHERE id=target FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'Current wallet unavailable; retained original claims remain separate'; END IF;
 SELECT * INTO c FROM commercial_cases WHERE id=c.id FOR UPDATE;
 IF NOT commercial_personal_proof(c.id,p_body) OR NOT (
  (target=c.subject AND NOT commercial_is_learning_subject(target)) OR EXISTS(
   SELECT 1 FROM commercial_learning_subjects l WHERE l.subject=target AND l.case_id=c.id AND l.erased_at IS NULL)) THEN
  RAISE EXCEPTION 'Claimant proof or selected wallet changed while waiting';
 END IF;
 PERFORM commercial_preserve_records(c.id,c.subject);
 SELECT * INTO wallet FROM coins WHERE user_id=target FOR UPDATE;
 IF wallet.user_id IS NULL OR wallet.coins<amount THEN RAISE EXCEPTION 'Insufficient available wallet value; withheld units are separate'; END IF;
 captured:=commercial_captured_units(c.id); capacity:=commercial_cash_capacity(c.id);
 IF (captured IS NOT NULL AND amount>captured) OR (capacity IS NOT NULL AND amount>capacity) THEN
  RAISE EXCEPTION 'Requested amount exceeds the currently known unreserved purchased-value ceiling';
 END IF;
 observed_basis:=jsonb_build_object('captured_purchase_units',captured,'unreserved_purchase_capacity',capacity,
  'historic_refund_review',(SELECT to_jsonb(b) FROM commercial_cash_basis b WHERE b.case_id=c.id),
  'scope','Observed request-time claimant ceiling, not a cash payment or guaranteed final eligibility; no coin lots inferred');
 ledger:=gen_random_uuid(); obligation:=gen_random_uuid();
 original:=jsonb_build_object('kind','existing_wallet_value_cash_assessment_request','command_id',command,
  'wallet_subject',target,'ledger_id',ledger,'units',amount,'method_requested','cash',
  'available_before',wallet.coins,'available_after',wallet.coins-amount,
  'withheld_before',wallet.withheld_coins,'withheld_after',wallet.withheld_coins,
  'basis',observed_basis,'policy','fungible_reward_first_100_coins_per_euro',
  'cash_eligibility','requires_supported_assessment_and_verified_destination',
  'cash_paid',false,'new_value_issued',false);
 UPDATE coins SET coins=coins-amount WHERE user_id=target;
 INSERT INTO transactions(id,user_id,created_at,coins,description,include_in_credit_note)
 VALUES(ledger,target,clock_timestamp(),-amount,'Existing wallet value reserved for requested cash assessment',false);
 INSERT INTO commercial_obligations(id,case_id,source,source_key,component,units,cash_units,status,original)
 VALUES(obligation,c.id,'backend',ledger::text,'active_wallet_cash',amount,NULL,'established',original);
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 VALUES(c.id,'wallet_cash_request',command::text,original||jsonb_build_object('original_ledger',(
  SELECT to_jsonb(t) FROM transactions t WHERE t.id=ledger)));
 UPDATE commercial_cases SET closed_at=NULL,review_due_at=least(review_due_at,clock_timestamp()) WHERE id=c.id;
 result:=jsonb_build_object('request_id',command,'obligation_id',obligation,'ledger_id',ledger,
  'wallet_subject',target,'units_moved_to_claim',amount,'wallet_balance',wallet.coins-amount,
  'existing_value_preserved',true,'cash_eligibility_review_required',true,
  'cash_paid',false,'payment_execution_performed',false,'claims_satisfied',false,
  'coin_return_right_preserved_for_unreserved_value',true);
 INSERT INTO commercial_journal(case_id,obligation_id,actor,command_id,kind,request,result)
 VALUES(c.id,obligation,p_actor,command,'request_wallet_cash',payload,result);
 RETURN result;
END $$;

CREATE FUNCTION commercial_wallet_cash_return(p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c commercial_cases; receipt commercial_journal; o commercial_obligations;
 command uuid:=(p_body->>'command_id')::uuid; target uuid:=(p_body->>'wallet_subject')::uuid;
 requested_obligation uuid:=(p_body->>'obligation_id')::uuid; amount bigint:=(p_body->>'units')::bigint;
 payload jsonb:=p_body-'_claim_hash'-'_moderation_hash'; ledger uuid; balance bigint; result jsonb;
BEGIN
 IF command IS NULL OR target IS NULL OR requested_obligation IS NULL OR amount IS NULL OR amount<=0
  OR jsonb_typeof(p_body->'units') IS DISTINCT FROM 'number' OR p_body->'choose_coins' IS DISTINCT FROM 'true'::jsonb
  OR payload-'command_id'-'wallet_subject'-'obligation_id'-'units'-'choose_coins'<>'{}'::jsonb THEN
  RAISE EXCEPTION 'Exact wallet claim, available amount and explicit coin-return election required';
 END IF;
 SELECT * INTO c FROM commercial_cases WHERE subject=p_actor;
 IF c.id IS NULL OR NOT commercial_personal_proof(c.id,p_body) THEN RAISE EXCEPTION 'Current personal claimant proof required'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF NOT commercial_personal_proof(c.id,p_body) THEN RAISE EXCEPTION 'Claimant proof changed while waiting for original return'; END IF;
 SELECT * INTO receipt FROM commercial_journal WHERE command_id=command;
 IF FOUND THEN
  IF receipt.case_id IS DISTINCT FROM c.id OR receipt.actor IS DISTINCT FROM p_actor
   OR receipt.kind<>'return_wallet_cash' OR receipt.request<>payload THEN RAISE EXCEPTION 'Conflicting wallet-value return'; END IF;
  RETURN receipt.result;
 END IF;
 SELECT * INTO o FROM commercial_obligations WHERE id=requested_obligation AND case_id=c.id;
 IF o.id IS NULL OR o.component<>'active_wallet_cash' OR o.source<>'backend' THEN RAISE EXCEPTION 'Original wallet cash-assessment claim required'; END IF;
 -- The original still-live wallet or an explicitly selected active limited
 -- subject of the same claimant can receive its own previously spendable value.
 IF NOT ((target=c.subject AND target=(o.original->>'wallet_subject')::uuid AND NOT commercial_is_learning_subject(target)) OR EXISTS(
  SELECT 1 FROM commercial_learning_subjects l WHERE l.subject=target AND l.case_id=c.id AND l.erased_at IS NULL)) THEN
  RAISE EXCEPTION 'Current owned destination for this existing wallet claim required';
 END IF;
 PERFORM 1 FROM users WHERE id=target FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'Selected wallet no longer exists; original claim remains preserved'; END IF;
 -- Reversing to the original ordinary source wallet preserves that wallet's
 -- existing rights. Applying to a limited subject is a usable-credit return:
 -- its ordinary-auth absence is expected, but current learning admission must
 -- actually be available. Preserve the claim unchanged while it is restricted.
 IF commercial_is_learning_subject(target) AND NOT commercial_purchase_lock(target) THEN
  RAISE EXCEPTION 'Selected limited access is unavailable; the unreturned claim remains preserved';
 END IF;
 SELECT * INTO c FROM commercial_cases WHERE id=c.id FOR UPDATE;
 SELECT * INTO o FROM commercial_obligations WHERE id=requested_obligation AND case_id=c.id FOR UPDATE;
 IF NOT commercial_personal_proof(c.id,p_body) OR NOT (
  (target=c.subject AND target=(o.original->>'wallet_subject')::uuid AND NOT commercial_is_learning_subject(target)) OR EXISTS(
   SELECT 1 FROM commercial_learning_subjects l WHERE l.subject=target AND l.case_id=c.id AND l.erased_at IS NULL)) THEN
  RAISE EXCEPTION 'Claimant proof or selected wallet changed while waiting';
 END IF;
 IF o.status<>'established' OR amount>commercial_remaining(o.id) THEN RAISE EXCEPTION 'Only established unreserved wallet value can be returned'; END IF;
 ledger:=gen_random_uuid();
 INSERT INTO commercial_reservations(id,obligation_id,units,mode,state,request,outcome,completed_at)
 VALUES(command,o.id,amount,'wallet','completed',payload,jsonb_build_object(
  'subject',target,'ledger_id',ledger,'kind','existing_wallet_cash_request_return','new_value_issued',false),clock_timestamp());
 INSERT INTO coins(user_id,coins,withheld_coins) VALUES(target,amount,0)
 ON CONFLICT(user_id) DO UPDATE SET coins=coins.coins+excluded.coins RETURNING coins INTO balance;
 INSERT INTO transactions(id,user_id,created_at,coins,description,include_in_credit_note)
 VALUES(ledger,target,clock_timestamp(),amount,'Return of existing wallet value from cash-assessment claim',false);
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 VALUES(c.id,'wallet_cash_return',command::text,jsonb_build_object('obligation_id',o.id,'reservation_id',command,
  'wallet_subject',target,'units',amount,'original_request',o.original,'original_ledger',(
   SELECT to_jsonb(t) FROM transactions t WHERE t.id=ledger),'cash_paid',false));
 result:=jsonb_build_object('obligation_id',o.id,'reservation_id',command,'ledger_id',ledger,
  'wallet_subject',target,'units_returned',amount,'wallet_balance',balance,'cash_paid',false,
  'remaining_obligation_units',commercial_remaining(o.id),'ordinary_authority_created',false,'new_value_issued',false);
 INSERT INTO commercial_journal(case_id,obligation_id,actor,command_id,kind,request,result)
 VALUES(c.id,o.id,p_actor,command,'return_wallet_cash',payload,result);
 RETURN result;
END $$;

CREATE OR REPLACE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation='request_wallet_cash' THEN RETURN commercial_wallet_cash_request(p_actor,p_body); END IF;
 IF p_operation='return_wallet_cash' THEN RETURN commercial_wallet_cash_return(p_actor,p_body); END IF;
 IF p_operation IN ('restore_credit','cash_basis_review') THEN RETURN commercial_value_operation(p_operation,p_actor,p_body); END IF;
 IF p_operation='purchase_authorize' THEN RETURN commercial_purchase_authorize(p_actor,p_body); END IF;
 IF p_operation IN ('learning_start','learning_access','learning_summary','learning_revoke','learning_authority') THEN RETURN commercial_learning_operation(p_operation,p_actor,p_body); END IF;
 RETURN commercial_claim_operation(p_operation,p_actor,p_body);
END $$;
