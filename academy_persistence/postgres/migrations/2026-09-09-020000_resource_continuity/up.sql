-- A fresh service identity continues the claimant's actual resources. It does
-- not manufacture another daily refill or convert observations into paid lots.
CREATE TABLE commercial_resource_continuations (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), case_id uuid NOT NULL REFERENCES commercial_cases(id),
 kind text NOT NULL CHECK(kind IN ('premium','heart_counter')),
 source_evidence uuid NOT NULL REFERENCES commercial_evidence(id), source_subject uuid NOT NULL,
 subject uuid NOT NULL REFERENCES commercial_learning_subjects(subject), command_id uuid NOT NULL UNIQUE,
 original jsonb NOT NULL, election jsonb NOT NULL, result jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), state text NOT NULL CHECK(state IN ('active','withdrawn')),
 UNIQUE(kind,source_evidence,subject)
);
CREATE FUNCTION commercial_resource_original_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF to_jsonb(NEW)-'state' IS DISTINCT FROM to_jsonb(OLD)-'state' THEN
  RAISE EXCEPTION 'Original resource observation and continuation are immutable'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_resource_original_guard BEFORE UPDATE ON commercial_resource_continuations
 FOR EACH ROW EXECUTE FUNCTION commercial_resource_original_guard();

CREATE FUNCTION commercial_continue_heart_counter() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE l commercial_learning_subjects; e commercial_evidence; amount bigint; refill timestamptz;
BEGIN
 SELECT * INTO l FROM commercial_learning_subjects WHERE subject=NEW.id;
 IF NOT FOUND THEN RETURN NEW; END IF;
 SELECT evidence.* INTO e FROM commercial_evidence evidence JOIN commercial_subject_erasures erasure
 ON erasure.subject::text=evidence.source_key AND erasure.case_id=evidence.case_id
 WHERE evidence.case_id=l.case_id AND evidence.category='heart_balance_at_erasure'
 ORDER BY erasure.erased_at DESC,evidence.recorded_at DESC,evidence.id DESC LIMIT 1;
 IF NOT FOUND THEN RETURN NEW; END IF; -- missing historical paid allocation remains unknown
 amount:=(e.evidence->>'hearts')::bigint;refill:=(e.evidence->>'last_refill')::timestamptz;
 IF amount IS NULL OR amount<0 OR refill IS NULL THEN RAISE EXCEPTION 'Actual prior heart counter/refill observation required'; END IF;
 INSERT INTO hearts(user_id,hearts,last_refill) VALUES(NEW.id,amount,refill);
 INSERT INTO commercial_resource_continuations(case_id,kind,source_evidence,source_subject,subject,command_id,original,election,result,state)
 VALUES(l.case_id,'heart_counter',e.id,e.source_key::uuid,NEW.id,gen_random_uuid(),e.evidence,
  jsonb_build_object('learning_election',l.election_id,'scope','Continue actual counter; no purchase or historical paid allocation inferred'),
  jsonb_build_object('initial_counter',amount,'last_refill',refill,'new_free_refill',false,'new_purchase',false,
   'daily_refill_rule','Existing configured HeartService decision remains authoritative','paid_refund_satisfied',false),'active');
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_continue_heart_counter AFTER INSERT ON users FOR EACH ROW EXECUTE FUNCTION commercial_continue_heart_counter();

CREATE FUNCTION commercial_resource_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c uuid; e commercial_evidence; prior commercial_resource_continuations;
 command uuid:=(p_body->>'command_id')::uuid; target uuid:=(p_body->>'successor')::uuid;
 payload jsonb:=p_body-'_claim_hash'-'_moderation_hash'; record_id uuid:=gen_random_uuid(); start_at timestamptz; end_at timestamptz;
BEGIN
 SELECT id INTO c FROM commercial_cases WHERE subject=p_actor;
 IF c IS NULL OR NOT commercial_personal_proof(c,p_body) THEN RAISE EXCEPTION 'Current personal claimant proof required'; END IF;
 IF p_operation='resource_rights' THEN
  RETURN jsonb_build_object('observations',coalesce((SELECT jsonb_agg(to_jsonb(x) ORDER BY x.recorded_at,x.id) FROM commercial_evidence x
    WHERE x.case_id=c AND x.category IN ('premium_right','heart_balance_at_erasure')),'[]'),
   'continuations',coalesce((SELECT jsonb_agg(to_jsonb(x) ORDER BY x.created_at,x.id) FROM commercial_resource_continuations x WHERE x.case_id=c),'[]'),
   'paid_allocation_inferred',false);
 END IF;
 IF p_operation<>'premium_continue' OR command IS NULL THEN RAISE EXCEPTION 'Exact Premium continuation election required'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF NOT commercial_personal_proof(c,p_body) THEN RAISE EXCEPTION 'Claimant proof changed while waiting'; END IF;
 SELECT * INTO prior FROM commercial_resource_continuations WHERE command_id=command;
 IF FOUND THEN
  IF prior.case_id<>c OR prior.election<>payload THEN RAISE EXCEPTION 'Conflicting original resource continuation'; END IF;
  RETURN to_jsonb(prior);
 END IF;
 IF EXISTS(SELECT 1 FROM commercial_journal WHERE command_id=command) THEN RAISE EXCEPTION 'Conflicting original command'; END IF;
 PERFORM commercial_purchase_lock(target);
 PERFORM 1 FROM commercial_cases WHERE id=c FOR UPDATE;
 SELECT * INTO e FROM commercial_evidence WHERE id=(p_body->>'evidence_id')::uuid AND case_id=c AND category='premium_right';
 IF e.id IS NULL OR NOT commercial_personal_proof(c,p_body)
  OR NOT EXISTS(SELECT 1 FROM commercial_learning_subjects WHERE subject=target AND case_id=c AND erased_at IS NULL)
  OR NOT EXISTS(SELECT 1 FROM commercial_subject_erasures WHERE subject=(e.evidence->>'subject')::uuid AND case_id=c)
  OR p_body->'continue_existing_right' IS DISTINCT FROM 'true'::jsonb
  OR p_body-'_claim_hash'-'_moderation_hash'-'command_id'-'successor'-'evidence_id'-'continue_existing_right'<>'{}'::jsonb
 THEN RAISE EXCEPTION 'Observed original Premium period and exact current successor election required'; END IF;
 start_at:=(e.evidence->>'since')::timestamptz;end_at:=(e.evidence->>'until')::timestamptz;
 IF start_at IS NULL OR end_at IS NULL OR start_at>clock_timestamp() OR end_at<=clock_timestamp() THEN
  RAISE EXCEPTION 'Original period is not currently deliverable; existing evidence and applicable resolution remain'; END IF;
 -- Keep every independently existing target period unchanged. This derived
 -- access row has exactly the original dates, not an added duration or renewal.
 INSERT INTO premium(id,user_id,since,until) VALUES(record_id,target,start_at,end_at);
 INSERT INTO commercial_resource_continuations(case_id,kind,source_evidence,source_subject,subject,command_id,original,election,result,state)
 VALUES(c,'premium',e.id,(e.evidence->>'subject')::uuid,target,command,e.evidence,payload,
  jsonb_build_object('period_id',record_id,'since',start_at,'until',end_at,'new_purchase',false,'renewal_activated',false,
   'original_period_id',e.evidence->>'id','current_access_granted',true),'active') RETURNING * INTO prior;
 INSERT INTO commercial_journal(command_id,case_id,kind,actor,request,result)
 VALUES(command,c,'premium_continue',p_actor,payload,to_jsonb(prior));
 RETURN to_jsonb(prior);
END $$;

CREATE FUNCTION commercial_withdraw_resource_continuation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 UPDATE commercial_resource_continuations SET state='withdrawn' WHERE subject=NEW.subject AND state='active';
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_withdraw_resource_continuation AFTER INSERT ON commercial_subject_erasures
 FOR EACH ROW EXECUTE FUNCTION commercial_withdraw_resource_continuation();

ALTER FUNCTION commercial_operation(text,uuid,jsonb) RENAME TO commercial_operation_before_resource_continuity;
CREATE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation IN ('resource_rights','premium_continue') THEN RETURN commercial_resource_operation(p_operation,p_actor,p_body); END IF;
 RETURN commercial_operation_before_resource_continuity(p_operation,p_actor,p_body);
END $$;
ALTER FUNCTION commercial_case_export(uuid) RENAME TO commercial_case_export_before_resource_continuity;
CREATE FUNCTION commercial_case_export(p_subject uuid) RETURNS jsonb LANGUAGE sql AS $$
 SELECT commercial_case_export_before_resource_continuity(p_subject)||jsonb_build_object('resource_continuations',coalesce((
 SELECT jsonb_agg(to_jsonb(r) ORDER BY r.created_at,r.id) FROM commercial_resource_continuations r JOIN commercial_cases c ON c.id=r.case_id
 WHERE c.subject=p_subject),'[]'));
$$;
