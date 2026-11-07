-- An administrator lets the unpaid remainder of an erased account's wallet
-- obligation lapse after the owner of the platform decided so. Only value
-- that was established as never paid (cash_units 0) and never reserved can
-- lapse; purchased coins and any payment or wallet transfer keep their own
-- paths. The obligation becomes 'rejected', so commercial_remaining is 0 and
-- the case leaves the due part of the staff queue. The earlier determination
-- stays in the new determination record and in the journal.
CREATE FUNCTION commercial_forfeit_unpaid_remainder(p_actor uuid,p_body jsonb)
RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE
 request_body jsonb; command uuid; c uuid; owner_id uuid; old_command commercial_journal;
 obligation commercial_obligations; amount bigint; result jsonb; field text;
BEGIN
 IF current_setting('transaction_isolation')<>'read committed' THEN
  RAISE EXCEPTION 'Forfeiture requires READ COMMITTED';
 END IF;
 IF p_body IS NULL OR jsonb_typeof(p_body)<>'object' THEN RAISE EXCEPTION 'Object request required'; END IF;
 request_body:=p_body-'_claim_hash'-'_moderation_hash'-'_staff_session'-'_staff_refresh_hash';
 IF jsonb_typeof(p_body->'command_id') IS DISTINCT FROM 'string' THEN RAISE EXCEPTION 'Exact command identity required'; END IF;
 command:=(p_body->>'command_id')::uuid;
 IF p_actor IS NULL THEN RAISE EXCEPTION 'Proved actor required'; END IF;
 IF jsonb_typeof(p_body->'assessment') IS DISTINCT FROM 'string' OR length(trim(p_body->>'assessment'))<20 THEN
  RAISE EXCEPTION 'Specific human assessment required';
 END IF;
 IF jsonb_typeof(p_body->'owner_decision') IS DISTINCT FROM 'string' OR length(trim(p_body->>'owner_decision'))<20 THEN
  RAISE EXCEPTION 'Documented owner decision required';
 END IF;
 c:=(p_body->>'case_id')::uuid;
 SELECT subject INTO owner_id FROM commercial_cases WHERE id=c;
 IF owner_id IS NULL THEN RAISE EXCEPTION 'Commercial recipient required'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Current administrator MFA authority required'; END IF;
 SELECT * INTO old_command FROM commercial_journal WHERE command_id=command;
 IF FOUND THEN
  IF old_command.actor IS DISTINCT FROM p_actor OR old_command.kind<>'forfeit' OR old_command.request<>request_body THEN
   RAISE EXCEPTION 'Conflicting commercial command replay';
  END IF;
  RETURN old_command.result;
 END IF;

 IF (SELECT count(*) FROM jsonb_object_keys(request_body))<>7 OR
  NOT request_body ?& ARRAY['command_id','case_id','subject','obligation_id','units','assessment','owner_decision'] THEN
  RAISE EXCEPTION 'Exact forfeiture fields required';
 END IF;
 FOREACH field IN ARRAY ARRAY['case_id','subject','obligation_id'] LOOP
  IF jsonb_typeof(request_body->field) IS DISTINCT FROM 'string' THEN RAISE EXCEPTION 'UUID strings required'; END IF;
  PERFORM (request_body->>field)::uuid;
 END LOOP;
 IF (request_body->>'subject')::uuid IS DISTINCT FROM owner_id THEN RAISE EXCEPTION 'Recipient case mismatch'; END IF;
 IF jsonb_typeof(request_body->'units') IS DISTINCT FROM 'string' OR (request_body->>'units')!~'^[1-9][0-9]{0,17}$' THEN
  RAISE EXCEPTION 'Canonical positive amount string required';
 END IF;
 amount:=(request_body->>'units')::bigint;

 PERFORM 1 FROM users WHERE id=owner_id FOR UPDATE;
 PERFORM 1 FROM commercial_cases WHERE id=c AND subject=owner_id FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'Recipient case mismatch'; END IF;
 SELECT * INTO obligation FROM commercial_obligations WHERE id=(request_body->>'obligation_id')::uuid AND case_id=c FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'Exact original obligation required'; END IF;
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Administrator authority changed while waiting'; END IF;
 IF NOT EXISTS(SELECT 1 FROM commercial_cases WHERE id=c AND erased_at IS NOT NULL) THEN
  RAISE EXCEPTION 'Only value of an erased account can lapse';
 END IF;
 IF obligation.source<>'backend' OR obligation.component NOT IN ('wallet_available','wallet_withheld') THEN
  RAISE EXCEPTION 'Only retained wallet value can lapse';
 END IF;
 IF obligation.status<>'established' OR obligation.cash_units IS DISTINCT FROM 0::bigint THEN
  RAISE EXCEPTION 'Only established value without any cash component can lapse';
 END IF;
 IF EXISTS(SELECT 1 FROM commercial_reservations WHERE obligation_id=obligation.id AND state<>'failed') THEN
  RAISE EXCEPTION 'Reserved or applied value cannot lapse';
 END IF;
 IF obligation.units IS DISTINCT FROM amount OR commercial_remaining(obligation.id) IS DISTINCT FROM amount THEN
  RAISE EXCEPTION 'Current remainder differs from observation';
 END IF;
 UPDATE commercial_obligations SET status='rejected',
  determination=jsonb_build_object('kind','forfeited_unpaid_remainder','forfeiture',request_body,'prior_determination',obligation.determination)
  WHERE id=obligation.id;
 result:=jsonb_build_object('obligation_id',obligation.id,'status','rejected','forfeited_units',amount::text,'paid',false);
 INSERT INTO commercial_journal(case_id,obligation_id,actor,command_id,kind,request,result)
 VALUES(c,obligation.id,p_actor,command,'forfeit',request_body,result);
 RETURN result;
END $$;

-- The retention-page dispatcher stays the top level; it routes forfeit as well.
CREATE OR REPLACE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation='admin_retention_page' THEN RETURN commercial_retention_page(p_actor,p_body); END IF;
 IF p_operation='forfeit' THEN RETURN commercial_forfeit_unpaid_remainder(p_actor,p_body); END IF;
 RETURN commercial_operation_before_retention_page(p_operation,p_actor,p_body);
END $$;
