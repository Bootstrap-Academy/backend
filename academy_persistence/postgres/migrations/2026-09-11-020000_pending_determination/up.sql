-- New decisions compare the current pending tuple. This is not a history or
-- first-ever-decision fence. Genuine original receipts keep their old admission.
CREATE FUNCTION commercial_pending_determination(p_actor uuid,p_body jsonb)
RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE
 request_body jsonb; command uuid; c uuid; owner_id uuid; target_owner uuid;
 old_command commercial_journal; obligation commercial_obligations;
 expected jsonb; amount bigint; cash bigint; expected_units bigint;
 expected_cash bigint; expected_determination jsonb; result jsonb; field text;
BEGIN
 IF current_setting('transaction_isolation')<>'read committed' THEN
  RAISE EXCEPTION 'Pending determination requires READ COMMITTED';
 END IF;
 IF p_body IS NULL OR jsonb_typeof(p_body)<>'object' THEN RAISE EXCEPTION 'Object request required'; END IF;
 request_body:=p_body-'_claim_hash'-'_moderation_hash'-'_staff_session'-'_staff_refresh_hash';
 IF jsonb_typeof(p_body->'command_id') IS DISTINCT FROM 'string' THEN RAISE EXCEPTION 'Exact command identity required'; END IF;
 command:=(p_body->>'command_id')::uuid;
 IF p_actor IS NULL THEN RAISE EXCEPTION 'Proved actor required'; END IF;
 IF jsonb_typeof(p_body->'assessment') IS DISTINCT FROM 'string' OR length(trim(p_body->>'assessment'))<20 THEN
  RAISE EXCEPTION 'Specific human assessment required';
 END IF;
 c:=(p_body->>'case_id')::uuid;
 SELECT subject INTO owner_id FROM commercial_cases WHERE id=c;
 IF owner_id IS NULL THEN RAISE EXCEPTION 'Commercial recipient required'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Current administrator MFA authority required'; END IF;
 SELECT * INTO old_command FROM commercial_journal WHERE command_id=command;
 IF FOUND THEN
  IF old_command.actor IS DISTINCT FROM p_actor OR old_command.kind<>'determine' OR old_command.request<>request_body THEN
   RAISE EXCEPTION 'Conflicting commercial command replay';
  END IF;
  RETURN old_command.result;
 END IF;

 IF (SELECT count(*) FROM jsonb_object_keys(request_body))<>9 OR
  NOT request_body ?& ARRAY['command_id','case_id','obligation_id','units','cash_units','assessment','evidence','subject','expected_obligation'] THEN
  RAISE EXCEPTION 'Exact new determination fields required';
 END IF;
 FOREACH field IN ARRAY ARRAY['case_id','subject','obligation_id'] LOOP
  IF jsonb_typeof(request_body->field) IS DISTINCT FROM 'string' THEN RAISE EXCEPTION 'UUID strings required'; END IF;
  PERFORM (request_body->>field)::uuid;
 END LOOP;
 target_owner:=(request_body->>'subject')::uuid;
 IF target_owner IS DISTINCT FROM owner_id THEN RAISE EXCEPTION 'Recipient case mismatch'; END IF;
 FOREACH field IN ARRAY ARRAY['units','cash_units'] LOOP
  IF field='cash_units' AND request_body->field='null'::jsonb THEN CONTINUE; END IF;
  IF jsonb_typeof(request_body->field) IS DISTINCT FROM 'string' OR (request_body->>field)!~'^(0|[1-9][0-9]*)$' THEN
   RAISE EXCEPTION 'Canonical nonnegative amount strings required';
  END IF;
  BEGIN PERFORM (request_body->>field)::bigint;
  EXCEPTION WHEN numeric_value_out_of_range THEN RAISE EXCEPTION 'Amount outside supported range'; END;
 END LOOP;
 amount:=(request_body->>'units')::bigint; cash:=(request_body->>'cash_units')::bigint;
 IF cash>amount OR jsonb_typeof(request_body->'evidence') IS DISTINCT FROM 'object' OR request_body->'evidence'='{}'::jsonb THEN
  RAISE EXCEPTION 'Amount, tender certainty and evidence required';
 END IF;
 IF cash IS NOT NULL AND length(coalesce(request_body->'evidence'->>'cash_basis',''))<20 THEN
  RAISE EXCEPTION 'Specific purchased-tender or other established cash basis required';
 END IF;
 expected:=request_body->'expected_obligation';
 IF jsonb_typeof(expected) IS DISTINCT FROM 'object' THEN RAISE EXCEPTION 'Exact pending observation required'; END IF;
 IF (SELECT count(*) FROM jsonb_object_keys(expected))<>4 OR NOT expected ?& ARRAY['status','units','cash_units','determination_json'] OR
  expected->>'status' IS DISTINCT FROM 'pending_evidence' THEN RAISE EXCEPTION 'Exact pending observation required'; END IF;
 FOREACH field IN ARRAY ARRAY['units','cash_units'] LOOP
  IF expected->field='null'::jsonb THEN CONTINUE; END IF;
  IF jsonb_typeof(expected->field) IS DISTINCT FROM 'string' OR (expected->>field)!~'^(0|[1-9][0-9]*)$' THEN
   RAISE EXCEPTION 'Canonical observed amount strings required';
  END IF;
  BEGIN PERFORM (expected->>field)::bigint;
  EXCEPTION WHEN numeric_value_out_of_range THEN RAISE EXCEPTION 'Observed amount outside supported range'; END;
 END LOOP;
 expected_units:=(expected->>'units')::bigint; expected_cash:=(expected->>'cash_units')::bigint;
 IF expected->'determination_json'='null'::jsonb THEN expected_determination:=NULL;
 ELSIF jsonb_typeof(expected->'determination_json')='string' THEN expected_determination:=(expected->>'determination_json')::jsonb;
 ELSE RAISE EXCEPTION 'Observed determination JSON text or null required'; END IF;

 -- Do not use commercial_lock_subject: it can create or follow another case.
 PERFORM 1 FROM users WHERE id=target_owner FOR UPDATE;
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Administrator authority changed while waiting for user'; END IF;
 PERFORM 1 FROM commercial_cases WHERE id=c AND subject=target_owner FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'Recipient case mismatch'; END IF;
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Administrator authority changed while waiting for case'; END IF;
 SELECT * INTO obligation FROM commercial_obligations WHERE id=(request_body->>'obligation_id')::uuid AND case_id=c FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'Exact original obligation required'; END IF;
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Administrator authority changed while waiting for obligation'; END IF;
 IF obligation.status<>'pending_evidence' OR obligation.units IS DISTINCT FROM expected_units OR
  obligation.cash_units IS DISTINCT FROM expected_cash OR obligation.determination IS DISTINCT FROM expected_determination THEN
  RAISE EXCEPTION 'Current pending obligation differs from observation';
 END IF;
 IF obligation.units IS NOT NULL AND amount<>obligation.units THEN RAISE EXCEPTION 'Original amount immutable; use a separately linked adjustment'; END IF;
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Current administrator MFA authority required before determination'; END IF;
 UPDATE commercial_obligations SET units=amount,cash_units=cash,status='established',determination=request_body,review_due_at=clock_timestamp() WHERE id=obligation.id;
 result:=jsonb_build_object('obligation_id',obligation.id,'status','established','paid',false);
 INSERT INTO commercial_journal(case_id,obligation_id,actor,command_id,kind,request,result)
 VALUES(c,obligation.id,p_actor,command,'determine',request_body,result);
 RETURN result;
END $$;

-- Validate only the selected immutable journal, with the original admission
-- casts. No current row comparison or stricter new-body gate applies to it.
CREATE FUNCTION commercial_determination_journal(p_j commercial_journal)
RETURNS jsonb LANGUAGE plpgsql IMMUTABLE SET DateStyle='ISO,YMD' SET TimeZone='UTC' AS $$
DECLARE r jsonb:=p_j.request; amount bigint; cash bigint;
BEGIN
 IF p_j.actor IS NULL OR p_j.kind<>'determine' OR p_j.case_id IS NULL OR p_j.obligation_id IS NULL OR
  jsonb_typeof(r) IS DISTINCT FROM 'object' OR r IS DISTINCT FROM r-'_claim_hash'-'_moderation_hash'-'_staff_session'-'_staff_refresh_hash' OR
  jsonb_typeof(r->'command_id') IS DISTINCT FROM 'string' OR (r->>'command_id')::uuid IS DISTINCT FROM p_j.command_id OR
  (r->>'case_id')::uuid IS DISTINCT FROM p_j.case_id OR (r->>'obligation_id')::uuid IS DISTINCT FROM p_j.obligation_id OR
  jsonb_typeof(r->'assessment') IS DISTINCT FROM 'string' OR length(trim(r->>'assessment'))<20 THEN
  RAISE EXCEPTION USING ERRCODE='22000',MESSAGE='Unavailable determination history';
 END IF;
 amount:=(r->>'units')::bigint; cash:=nullif(r->>'cash_units','')::bigint;
 IF amount IS NULL OR amount<0 OR cash<0 OR cash>amount OR jsonb_typeof(r->'evidence') IS DISTINCT FROM 'object' OR r->'evidence'='{}'::jsonb OR
  (cash IS NOT NULL AND length(coalesce(r->'evidence'->>'cash_basis',''))<20) OR
  p_j.result IS DISTINCT FROM jsonb_build_object('obligation_id',p_j.obligation_id,'status','established','paid',false) THEN
  RAISE EXCEPTION USING ERRCODE='22000',MESSAGE='Unavailable determination history';
 END IF;
 RETURN jsonb_build_object('id',p_j.id::text,'case_id',p_j.case_id,'obligation_id',p_j.obligation_id,'actor',p_j.actor,
  'command_id',p_j.command_id,'kind',p_j.kind,'request_json',r::text,'result_json',p_j.result::text,'recorded_at',p_j.recorded_at::text);
EXCEPTION WHEN OTHERS THEN
 RAISE EXCEPTION USING ERRCODE='22000',MESSAGE='Unavailable determination history';
END $$;

CREATE FUNCTION commercial_determination_status(p_actor uuid,p_body jsonb)
RETURNS jsonb LANGUAGE plpgsql SET DateStyle='ISO,YMD' SET TimeZone='UTC' AS $$
DECLARE result jsonb;
BEGIN
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Current administrator MFA authority required'; END IF;
 -- One bounded local statement; no capture, lock, live-user or open-case gate.
 SELECT jsonb_build_object('protocol',1,'case_id',c.id,'subject',c.subject,'observed_at',clock_timestamp()::text,
  'obligation',jsonb_build_object('id',o.id,'source',o.source,'source_key',o.source_key,'component',o.component,'status',o.status,
   'units',o.units::text,'cash_units',o.cash_units::text,'original_json',o.original::text,'determination_json',o.determination::text),
  'journal',CASE WHEN j.command_id IS NULL THEN NULL ELSE commercial_determination_journal(j) END)
 INTO result FROM commercial_cases c JOIN commercial_obligations o ON o.case_id=c.id
 LEFT JOIN commercial_journal j ON j.case_id=c.id AND j.obligation_id=o.id AND j.command_id=(p_body->>'command_id')::uuid AND j.kind='determine'
 WHERE c.id=(p_body->>'case_id')::uuid AND c.subject=(p_body->>'subject')::uuid AND o.id=(p_body->>'obligation_id')::uuid;
 RETURN result;
END $$;

ALTER FUNCTION commercial_operation(text,uuid,jsonb) RENAME TO commercial_operation_before_pending_determination;
CREATE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation='determine' THEN RETURN commercial_pending_determination(p_actor,p_body); END IF;
 IF p_operation='admin_determination_status' THEN RETURN commercial_determination_status(p_actor,p_body); END IF;
 RETURN commercial_operation_before_pending_determination(p_operation,p_actor,p_body);
END $$;
