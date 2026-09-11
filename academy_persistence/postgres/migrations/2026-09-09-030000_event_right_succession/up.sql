-- Explicit same-contract Event continuation; no new purchase, term, price or period.
CREATE FUNCTION commercial_event_successor(p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c uuid; target uuid:=(p_body->>'successor')::uuid; source_subject uuid:=(p_body->>'source_subject')::uuid;
 command uuid:=(p_body->>'command_id')::uuid; original jsonb:=p_body->'original_scope'; grant_row commercial_successor_grants;
 payload jsonb:=p_body-'_claim_hash'-'_moderation_hash';
BEGIN
 SELECT id INTO c FROM commercial_cases WHERE subject=p_actor;
 IF command IS NULL OR c IS NULL OR NOT commercial_personal_proof(c,p_body) THEN
  RAISE EXCEPTION 'Exact personal claimant continuation election required'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF NOT commercial_personal_proof(c,p_body) THEN RAISE EXCEPTION 'Claimant proof changed while waiting'; END IF;
 SELECT * INTO grant_row FROM commercial_successor_grants WHERE command_id=command;
 IF FOUND THEN
  IF grant_row.case_id<>c OR grant_row.claimant_authorization<>payload THEN RAISE EXCEPTION 'Conflicting original continuation command'; END IF;
  RETURN to_jsonb(grant_row);
 END IF;
 IF EXISTS(SELECT 1 FROM commercial_journal WHERE command_id=command) THEN RAISE EXCEPTION 'Conflicting original command'; END IF;
 PERFORM commercial_purchase_lock(target);
 PERFORM 1 FROM commercial_cases WHERE id=c FOR UPDATE;
 IF NOT commercial_personal_proof(c,p_body)
  OR NOT EXISTS(SELECT 1 FROM commercial_learning_subjects WHERE subject=target AND case_id=c AND erased_at IS NULL)
  OR NOT commercial_owned_service_subject(p_actor,source_subject)
  OR NOT EXISTS(SELECT 1 FROM commercial_subject_erasures WHERE subject=source_subject AND case_id=c)
  OR p_body->'continue_existing_right' IS DISTINCT FROM 'true'::jsonb
  OR jsonb_typeof(original) IS DISTINCT FROM 'object'
  OR original->>'id' IS DISTINCT FROM p_body->>'right_id'
  OR original->>'source_subject' IS DISTINCT FROM source_subject::text
  OR p_body-'_claim_hash'-'_moderation_hash'-'command_id'-'successor'-'source_subject'-'right_id'-'continue_existing_right'-'original_scope'<>'{}'::jsonb
 THEN RAISE EXCEPTION 'Observed erased-source entitlement and exact current successor election required'; END IF;
 INSERT INTO commercial_successor_grants(id,case_id,source,original_contract,successor,command_id,original_scope,claimant_authorization,state)
 VALUES(command,c,'events',p_body->>'right_id',target,command,original,payload,'reserved') RETURNING * INTO grant_row;
 INSERT INTO commercial_journal(command_id,case_id,kind,actor,request,result)
 VALUES(command,c,'event_successor',p_actor,payload,to_jsonb(grant_row));
 RETURN to_jsonb(grant_row);
END $$;

CREATE FUNCTION commercial_event_successor_authority(p_body jsonb) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT to_jsonb(g)||jsonb_build_object('purpose','existing_event_continuation','new_purchase',false)
 FROM commercial_successor_grants g JOIN commercial_learning_subjects l ON l.subject=g.successor JOIN commercial_cases c ON c.id=g.case_id
 WHERE g.id=(p_body->>'grant_id')::uuid AND g.source='events' AND g.state IN ('reserved','uncertain','granted')
 AND g.claimant_authorization->>'source_subject'=p_body->>'source_subject'
 AND commercial_learning_allowed(g.successor)
 AND commercial_owned_service_subject(c.subject,(g.claimant_authorization->>'source_subject')::uuid);
$$;

CREATE FUNCTION commercial_event_successor_outcome(p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE g commercial_successor_grants; outcome jsonb:=p_body->'outcome'; state_value text;
BEGIN
 SELECT * INTO g FROM commercial_successor_grants WHERE id=(p_body->>'grant_id')::uuid;
 IF g.id IS NULL OR g.source<>'events' THEN RAISE EXCEPTION 'Original event continuation required'; END IF;
 PERFORM 1 FROM commercial_cases WHERE id=g.case_id FOR UPDATE;
 SELECT * INTO g FROM commercial_successor_grants WHERE id=g.id FOR UPDATE;
 state_value:=outcome->>'state';
 IF state_value NOT IN ('granted','withdrawn','rejected','uncertain')
  OR outcome->>'grant_id' IS DISTINCT FROM g.id::text
  OR outcome->>'subject' IS DISTINCT FROM g.successor::text
  OR outcome->>'right_id' IS DISTINCT FROM g.original_contract THEN RAISE EXCEPTION 'Exact event delivery result required'; END IF;
 -- Persist an exact observation even when access is already terminal. A
 -- received service assertion is not a new grant, COMMIT-time claim or money.
 INSERT INTO commercial_successor_receipts(grant_id,receipt,receipt_hash)
 VALUES(g.id,outcome,encode(sha256(convert_to(outcome::text,'UTF8')),'hex')) ON CONFLICT DO NOTHING;
 IF EXISTS(SELECT 1 FROM commercial_successor_receipts r WHERE r.grant_id=g.id
  AND r.receipt_hash=encode(sha256(convert_to(outcome::text,'UTF8')),'hex') AND r.receipt<>outcome) THEN
  RAISE EXCEPTION 'Conflicting exact service receipt'; END IF;
 IF g.state IN ('withdrawn','rejected') OR (g.state='granted' AND state_value IN ('uncertain','rejected')) THEN RETURN to_jsonb(g); END IF;
 -- An old success arriving after subject erasure describes original delivery,
 -- never current usable access or permission to resurrect the erased subject.
 IF EXISTS(SELECT 1 FROM commercial_subject_erasures WHERE subject=g.successor) AND state_value='granted' THEN state_value:='withdrawn'; END IF;
 UPDATE commercial_successor_grants SET state=state_value,result=outcome WHERE id=g.id RETURNING * INTO g;
 RETURN to_jsonb(g);
END $$;

ALTER FUNCTION commercial_operation(text,uuid,jsonb) RENAME TO commercial_operation_before_event_succession;
CREATE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation='event_successor' THEN RETURN commercial_event_successor(p_actor,p_body); END IF;
 IF p_operation='event_successor_authority' THEN RETURN commercial_event_successor_authority(p_body); END IF;
 IF p_operation='event_successor_outcome' THEN RETURN commercial_event_successor_outcome(p_body); END IF;
 RETURN commercial_operation_before_event_succession(p_operation,p_actor,p_body);
END $$;

ALTER FUNCTION commercial_case_export(uuid) RENAME TO commercial_case_export_before_event_succession;
CREATE FUNCTION commercial_case_export(p_subject uuid) RETURNS jsonb LANGUAGE sql AS $$
 SELECT commercial_case_export_before_event_succession(p_subject)||jsonb_build_object(
  'event_successor_grants',coalesce((SELECT jsonb_agg(to_jsonb(g) ORDER BY g.created_at,g.id)
    FROM commercial_successor_grants g JOIN commercial_cases c ON c.id=g.case_id
    WHERE c.subject=p_subject AND g.source='events'),'[]'),
  'event_successor_receipts',coalesce((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.received_at,r.id)
    FROM commercial_successor_receipts r JOIN commercial_successor_grants g ON g.id=r.grant_id
    JOIN commercial_cases c ON c.id=g.case_id WHERE c.subject=p_subject AND g.source='events'),'[]'),
  'course_successor_grants',coalesce((SELECT jsonb_agg(to_jsonb(g) ORDER BY g.created_at,g.id)
    FROM commercial_successor_grants g JOIN commercial_cases c ON c.id=g.case_id
    WHERE c.subject=p_subject AND g.source='skills'),'[]'),
  'course_successor_receipts',coalesce((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.received_at,r.id)
    FROM commercial_successor_receipts r JOIN commercial_successor_grants g ON g.id=r.grant_id
    JOIN commercial_cases c ON c.id=g.case_id WHERE c.subject=p_subject AND g.source='skills'),'[]'));
$$;
