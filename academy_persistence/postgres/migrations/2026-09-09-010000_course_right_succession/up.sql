-- Explicit continuation of an observed existing course right. Neither the
-- authorization nor delivery creates a purchase, new term, price or duration.
ALTER TABLE commercial_successor_grants DROP CONSTRAINT commercial_successor_grants_state_check;
ALTER TABLE commercial_successor_grants ADD CHECK(state IN ('reserved','uncertain','granted','withdrawn','rejected'));
CREATE FUNCTION commercial_successor_original_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF to_jsonb(NEW)-'state'-'result' IS DISTINCT FROM to_jsonb(OLD)-'state'-'result' THEN
  RAISE EXCEPTION 'Original continuation election and observed entitlement are immutable'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_successor_original_guard BEFORE UPDATE ON commercial_successor_grants
 FOR EACH ROW EXECUTE FUNCTION commercial_successor_original_guard();

CREATE FUNCTION commercial_course_successor(p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
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
  OR original->>'source_user_id' IS DISTINCT FROM source_subject::text
  OR p_body-'_claim_hash'-'_moderation_hash'-'command_id'-'successor'-'source_subject'-'right_id'-'continue_existing_right'-'original_scope'<>'{}'::jsonb
 THEN RAISE EXCEPTION 'Observed erased-source entitlement and exact current successor election required'; END IF;
 INSERT INTO commercial_successor_grants(id,case_id,source,original_contract,successor,command_id,original_scope,claimant_authorization,state)
 VALUES(command,c,'skills',p_body->>'right_id',target,command,original,payload,'reserved') RETURNING * INTO grant_row;
 INSERT INTO commercial_journal(command_id,case_id,kind,actor,request,result)
 VALUES(command,c,'course_successor',p_actor,payload,to_jsonb(grant_row));
 RETURN to_jsonb(grant_row);
END $$;

CREATE FUNCTION commercial_course_successor_authority(p_body jsonb) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT to_jsonb(g)||jsonb_build_object('purpose','existing_course_continuation','new_purchase',false)
 FROM commercial_successor_grants g JOIN commercial_learning_subjects l ON l.subject=g.successor JOIN commercial_cases c ON c.id=g.case_id
 WHERE g.id=(p_body->>'grant_id')::uuid AND g.source='skills' AND g.state IN ('reserved','uncertain','granted')
 AND g.claimant_authorization->>'source_subject'=p_body->>'source_subject'
 AND commercial_learning_allowed(g.successor)
 AND commercial_owned_service_subject(c.subject,(g.claimant_authorization->>'source_subject')::uuid);
$$;

-- Receipt arrival and current access are separate facts. A successful local
-- delivery can become known only after the target's access has been erased.
CREATE TABLE commercial_successor_receipts (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), grant_id uuid NOT NULL REFERENCES commercial_successor_grants(id),
 received_at timestamptz NOT NULL DEFAULT clock_timestamp(), receipt jsonb NOT NULL,
 receipt_hash text NOT NULL CHECK(receipt_hash ~ '^[0-9a-f]{64}$'), UNIQUE(grant_id,receipt_hash)
);
CREATE TRIGGER immutable_commercial_successor_receipt BEFORE UPDATE ON commercial_successor_receipts
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();

CREATE FUNCTION commercial_course_successor_outcome(p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE g commercial_successor_grants; outcome jsonb:=p_body->'outcome'; state_value text;
BEGIN
 SELECT * INTO g FROM commercial_successor_grants WHERE id=(p_body->>'grant_id')::uuid;
 IF g.id IS NULL OR g.source<>'skills' THEN RAISE EXCEPTION 'Original course continuation required'; END IF;
 PERFORM 1 FROM commercial_cases WHERE id=g.case_id FOR UPDATE;
 SELECT * INTO g FROM commercial_successor_grants WHERE id=g.id FOR UPDATE;
 state_value:=outcome->>'state';
 IF state_value NOT IN ('granted','withdrawn','rejected','uncertain')
  OR outcome->>'grant_id' IS DISTINCT FROM g.id::text
  OR outcome->>'subject' IS DISTINCT FROM g.successor::text
  OR outcome->>'right_id' IS DISTINCT FROM g.original_contract THEN RAISE EXCEPTION 'Exact course delivery result required'; END IF;
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

ALTER FUNCTION commercial_operation(text,uuid,jsonb) RENAME TO commercial_operation_before_course_succession;
CREATE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation='course_successor' THEN RETURN commercial_course_successor(p_actor,p_body); END IF;
 IF p_operation='course_successor_authority' THEN RETURN commercial_course_successor_authority(p_body); END IF;
 IF p_operation='course_successor_outcome' THEN RETURN commercial_course_successor_outcome(p_body); END IF;
 RETURN commercial_operation_before_course_succession(p_operation,p_actor,p_body);
END $$;

ALTER FUNCTION commercial_case_export(uuid) RENAME TO commercial_case_export_before_course_succession;
CREATE FUNCTION commercial_case_export(p_subject uuid) RETURNS jsonb LANGUAGE sql AS $$
 SELECT commercial_case_export_before_course_succession(p_subject)||jsonb_build_object(
  'course_successor_grants',coalesce((SELECT jsonb_agg(to_jsonb(g) ORDER BY g.created_at,g.id)
    FROM commercial_successor_grants g JOIN commercial_cases c ON c.id=g.case_id WHERE c.subject=p_subject),'[]'),
  'course_successor_receipts',coalesce((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.received_at,r.id)
    FROM commercial_successor_receipts r JOIN commercial_successor_grants g ON g.id=r.grant_id
    JOIN commercial_cases c ON c.id=g.case_id WHERE c.subject=p_subject),'[]'));
$$;

CREATE FUNCTION commercial_withdraw_erased_successor() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 UPDATE commercial_successor_grants SET state='withdrawn'
 WHERE successor=NEW.subject AND state IN ('reserved','uncertain','granted');
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_withdraw_erased_successor AFTER INSERT ON commercial_subject_erasures
 FOR EACH ROW EXECUTE FUNCTION commercial_withdraw_erased_successor();
