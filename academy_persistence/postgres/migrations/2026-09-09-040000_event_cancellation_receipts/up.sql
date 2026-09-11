-- A personally expressed cancellation is independent of learning/data erasure.
-- The immutable receipt commits before Events processing; retries retain its time.
CREATE TABLE commercial_event_cancellation_observations (
 command_id uuid NOT NULL REFERENCES commercial_journal(command_id),
 receipt_hash text NOT NULL, observed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 outcome jsonb NOT NULL, PRIMARY KEY(command_id,receipt_hash)
);
CREATE TRIGGER immutable_event_cancellation_observation BEFORE UPDATE ON commercial_event_cancellation_observations
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();

CREATE FUNCTION commercial_event_cancel(p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c uuid; command uuid:=(p_body->>'command_id')::uuid; source_subject uuid:=(p_body->>'source_subject')::uuid;
 received timestamptz:=clock_timestamp(); prior commercial_journal; payload jsonb:=p_body-'_claim_hash'-'_moderation_hash'; result jsonb;
BEGIN
 SELECT id INTO c FROM commercial_cases WHERE subject=p_actor;
 IF command IS NULL OR c IS NULL OR NOT commercial_personal_proof(c,p_body) THEN
  RAISE EXCEPTION 'Exact personal claimant cancellation declaration required'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF NOT commercial_personal_proof(c,p_body) THEN RAISE EXCEPTION 'Claimant proof changed while waiting'; END IF;
 SELECT * INTO prior FROM commercial_journal WHERE command_id=command;
 IF FOUND THEN
  IF prior.case_id<>c OR prior.kind<>'event_cancellation' OR prior.request<>payload THEN
   RAISE EXCEPTION 'Conflicting original cancellation command'; END IF;
  RETURN prior.result;
 END IF;
 PERFORM 1 FROM commercial_cases WHERE id=c FOR UPDATE;
 IF NOT commercial_personal_proof(c,p_body)
  OR NOT coalesce(commercial_owned_service_subject(p_actor,source_subject),false)
  OR payload->'cancel_identified_contract' IS DISTINCT FROM 'true'::jsonb
  OR jsonb_typeof(payload->'original_text') IS DISTINCT FROM 'string'
  OR length(btrim(payload->>'original_text')) NOT BETWEEN 1 AND 8000
  OR jsonb_typeof(payload->'right_id') IS DISTINCT FROM 'string'
  OR (payload->>'right_id')::uuid IS NULL
  OR payload-'command_id'-'source_subject'-'right_id'-'cancel_identified_contract'-'original_text'<>'{}'::jsonb
 THEN RAISE EXCEPTION 'Exact owned original right and actual cancellation declaration required'; END IF;
 result:=jsonb_build_object('protocol',1,'command_id',command,'case_id',c,'source_subject',source_subject,
  'right_id',(payload->>'right_id')::uuid,'purpose','cancel_identified_event_contract',
  'source','authenticated_claimant_declaration','received_at',received,'declaration',payload,
  'service_processing','pending','financial_satisfaction',false);
 INSERT INTO commercial_journal(command_id,case_id,kind,actor,request,result)
 VALUES(command,c,'event_cancellation',p_actor,payload,result);
 RETURN result;
END $$;

CREATE FUNCTION commercial_event_cancellation_authority(p_body jsonb) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT j.result FROM commercial_journal j WHERE j.command_id=(p_body->>'command_id')::uuid
 AND j.kind='event_cancellation' AND j.result->>'source_subject'=p_body->>'source_subject';
$$;

CREATE FUNCTION commercial_event_cancellation_pending() RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT coalesce(jsonb_agg(item),'[]') FROM (
  SELECT jsonb_build_object('command_id',j.command_id,'source_subject',j.result->>'source_subject','right_id',j.result->>'right_id') AS item
  FROM commercial_journal j WHERE j.kind='event_cancellation'
   AND NOT EXISTS(SELECT 1 FROM commercial_event_cancellation_observations o WHERE o.command_id=j.command_id
     AND o.outcome->>'state' IN ('applied','resolution_required'))
  ORDER BY coalesce((SELECT max(o.observed_at) FROM commercial_event_cancellation_observations o WHERE o.command_id=j.command_id),j.recorded_at),j.id LIMIT 100
 ) AS pending;
$$;

CREATE FUNCTION commercial_event_cancellation_outcome(p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE original commercial_journal; outcome jsonb:=p_body->'outcome'; digest text;
BEGIN
 SELECT * INTO original FROM commercial_journal WHERE command_id=(p_body->>'command_id')::uuid AND kind='event_cancellation';
 IF NOT FOUND OR jsonb_typeof(outcome) IS DISTINCT FROM 'object'
  OR outcome->>'command_id' IS DISTINCT FROM original.command_id::text
  OR (outcome->>'source_subject')::uuid IS DISTINCT FROM (original.result->>'source_subject')::uuid
  OR (outcome->>'right_id')::uuid IS DISTINCT FROM (original.result->>'right_id')::uuid
  OR coalesce(outcome->>'state','') NOT IN ('applied','resolution_required','uncertain')
  OR outcome->'financial_satisfaction' IS DISTINCT FROM 'false'::jsonb THEN
  RAISE EXCEPTION 'Exact cancellation service observation required'; END IF;
 digest:=encode(sha256(convert_to(outcome::text,'UTF8')),'hex');
 INSERT INTO commercial_event_cancellation_observations(command_id,receipt_hash,outcome)
 VALUES(original.command_id,digest,outcome) ON CONFLICT DO NOTHING;
 IF EXISTS(SELECT 1 FROM commercial_event_cancellation_observations AS o WHERE o.command_id=original.command_id AND o.receipt_hash=digest AND o.outcome IS DISTINCT FROM p_body->'outcome') THEN
  RAISE EXCEPTION 'Conflicting cancellation observation'; END IF;
 RETURN jsonb_build_object('declaration',original.result,'service_outcome',outcome,'financial_satisfaction',false);
END $$;

ALTER FUNCTION commercial_operation(text,uuid,jsonb) RENAME TO commercial_operation_before_event_cancellation;
CREATE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation='event_cancel' THEN RETURN commercial_event_cancel(p_actor,p_body); END IF;
 IF p_operation='event_cancellation_pending' THEN RETURN commercial_event_cancellation_pending(); END IF;
 IF p_operation='event_cancellation_authority' THEN RETURN commercial_event_cancellation_authority(p_body); END IF;
 IF p_operation='event_cancellation_outcome' THEN RETURN commercial_event_cancellation_outcome(p_body); END IF;
 RETURN commercial_operation_before_event_cancellation(p_operation,p_actor,p_body);
END $$;

ALTER FUNCTION commercial_case_export(uuid) RENAME TO commercial_case_export_before_event_cancellation;
CREATE FUNCTION commercial_case_export(p_subject uuid) RETURNS jsonb LANGUAGE sql AS $$
 SELECT commercial_case_export_before_event_cancellation(p_subject)||jsonb_build_object(
 'event_cancellation_declarations',coalesce((SELECT jsonb_agg(j.result ORDER BY j.recorded_at,j.id)
  FROM commercial_journal j JOIN commercial_cases c ON c.id=j.case_id WHERE c.subject=p_subject AND j.kind='event_cancellation'),'[]'),
 'event_cancellation_observations',coalesce((SELECT jsonb_agg(to_jsonb(o) ORDER BY o.observed_at,o.receipt_hash)
  FROM commercial_event_cancellation_observations o JOIN commercial_journal j ON j.command_id=o.command_id
  JOIN commercial_cases c ON c.id=j.case_id WHERE c.subject=p_subject),'[]'));
$$;
