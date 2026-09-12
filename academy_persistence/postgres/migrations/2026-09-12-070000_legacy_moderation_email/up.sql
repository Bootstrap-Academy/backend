-- New unpublished migration: default inbox, email only for requested access or important native changes.
CREATE OR REPLACE FUNCTION moderation_decide(p_actor uuid,p_request jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c moderation_cases; prior moderation_decisions; d uuid:=gen_random_uuid(); k uuid:=(p_request->>'request_key')::uuid;
    outcome text:=p_request->>'outcome'; effect text; deadline timestamptz; available timestamptz:=clock_timestamp();
    effect_before jsonb; measure_before jsonb;
    public jsonb; result jsonb; required text; historical boolean:=false; previous_sanctions integer; ladder_days integer; reviewed moderation_decisions; upheld moderation_holds;
BEGIN
    IF k IS NULL OR p_actor IS NULL THEN RAISE EXCEPTION 'Decision identity required'; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('moderation-request:'||p_actor||':'||k,0));
    SELECT * INTO prior FROM moderation_decisions WHERE actor=p_actor AND request_key=k;
    IF FOUND THEN
      IF (prior.request-'reviewed_content')<>(p_request-'reviewed_content') THEN RAISE EXCEPTION 'Conflicting decision replay'; END IF;
      RETURN prior.public_statement;
    END IF;
    SELECT * INTO c FROM moderation_cases WHERE id=(p_request->>'case_id')::uuid;
    IF NOT FOUND THEN RAISE EXCEPTION 'Case not found'; END IF;
    PERFORM moderation_lock_target(c.target_kind,c.target_id);
    PERFORM pg_advisory_xact_lock(hashtextextended('moderation:'||c.target_kind||':'||c.target_id,0));
    SELECT * INTO c FROM moderation_cases WHERE id=c.id FOR UPDATE;
    IF c.revision IS DISTINCT FROM (p_request->>'expected_revision')::integer THEN RAISE EXCEPTION 'Stale case revision'; END IF;
    -- Native comparison under the target lock; never a caller-supplied importance flag.
    SELECT jsonb_build_object('enabled',state->'enabled','removed',state->'removed',
      'retired',state->'retired','withdrawn',state->'withdrawn') INTO effect_before
      FROM (SELECT moderation_effect(c.target_kind,c.target_id) AS state) current_effect;
    SELECT jsonb_build_object('effect',h.effect,'active',h.active,'rescinded',h.rescinded,'ends_at',h.ends_at)
      INTO measure_before FROM moderation_holds h WHERE h.case_id=c.id;
    IF c.target_kind='subtask' AND (p_request->>'reviewed_content_revision')::bigint IS DISTINCT FROM (SELECT content_revision FROM moderation_targets WHERE kind=c.target_kind AND id=c.target_id) THEN RAISE EXCEPTION 'Content changed or was not reviewed; reload the exact target'; END IF;
    IF p_request ? 'complaint_id' THEN
      SELECT d.* INTO reviewed FROM moderation_complaints a JOIN moderation_decisions d ON d.id=a.decision_id
       WHERE a.id=(p_request->>'complaint_id')::uuid AND a.case_id=c.id AND a.outcome_decision IS NULL;
      IF NOT FOUND THEN RAISE EXCEPTION 'Open complaint not found'; END IF;
    ELSE SELECT * INTO reviewed FROM moderation_decisions WHERE id=c.latest_decision; END IF;
    IF outcome='uphold' THEN
      SELECT * INTO upheld FROM moderation_holds WHERE case_id=c.id;
      IF NOT FOUND AND (reviewed.id IS NULL OR reviewed.outcome NOT IN ('warn','restore','uphold')) THEN RAISE EXCEPTION 'No existing measure to uphold; use an explicit decision'; END IF;
      IF reviewed.outcome IN ('warn','restore') OR (reviewed.outcome='uphold' AND reviewed.public_statement->>'upheld_measure' IS NULL) THEN upheld.effect:=NULL;upheld.ends_at:=NULL; END IF;
    END IF;
    IF outcome IS NULL OR outcome NOT IN ('provisional','uphold','remove','retire','restore','warn','restrict','authority_start','authority_change','authority_end')
      THEN RAISE EXCEPTION 'Explicit decision outcome required'; END IF;
    IF (c.target_kind<>'subtask' AND outcome IN ('provisional','remove','retire')) OR (c.target_kind='subtask' AND outcome='restrict') THEN RAISE EXCEPTION 'Outcome unsupported for this target kind'; END IF;
    IF outcome='warn' AND EXISTS(SELECT 1 FROM moderation_holds WHERE case_id=c.id AND active) THEN RAISE EXCEPTION 'Release the existing hold explicitly before a warning'; END IF;
    IF (c.source='authority_order')<>(outcome LIKE 'authority_%') THEN RAISE EXCEPTION 'Authority holds need their supported order lifecycle'; END IF;
    FOREACH required IN ARRAY ARRAY['rationale','ground','rule_version','automation','scope','redress'] LOOP
      IF length(trim(coalesce(p_request->>required,'')))<3 OR length(p_request->>required)>16000 THEN RAISE EXCEPTION 'Specific recipient-safe decision fields required: %',required; END IF;
    END LOOP;
    IF c.source='authority_order' THEN
      available:=greatest(available,c.notice_after);
    END IF;
    IF c.source='authority_order' AND length(coalesce(p_request->>'order_event_evidence',''))<3 THEN RAISE EXCEPTION 'Supported authority event required'; END IF;
    IF p_request ? 'notify_after' AND p_request->>'notify_after' IS NOT NULL THEN
      IF c.source<>'authority_order' AND (p_request->>'notify_after')::timestamptz>clock_timestamp() THEN RAISE EXCEPTION 'Only an authority instruction may defer this notice'; END IF;
      available:=greatest(available,(p_request->>'notify_after')::timestamptz);
      IF c.source='authority_order' THEN UPDATE moderation_cases SET notice_after=available WHERE id=c.id; END IF;
    END IF;
    IF p_request ? 'complaint_id' AND (p_request->'human_review' IS DISTINCT FROM 'true'::jsonb OR length(trim(coalesce(p_request->>'review_assessment','')))<3) THEN RAISE EXCEPTION 'Documented human complaint assessment required'; END IF;
    IF c.target_kind='account' AND (outcome='restrict' OR (outcome='uphold' AND upheld.effect IS NOT NULL)) THEN
      FOREACH required IN ARRAY ARRAY['misconduct_facts','proportionality','hearing'] LOOP
       IF length(trim(coalesce(p_request->>required,'')))<3 THEN RAISE EXCEPTION 'Specific account grounds, proportionality and hearing or urgency assessment required'; END IF;
      END LOOP;
    END IF;
    IF c.target_kind IN ('create','report') AND (outcome='restrict' OR (outcome='uphold' AND upheld.effect IS NOT NULL)) THEN
      FOREACH required IN ARRAY ARRAY['misconduct_facts','proportionality','hearing'] LOOP
       IF length(trim(coalesce(p_request->>required,'')))<3 THEN RAISE EXCEPTION 'Specific misconduct, proportionality and hearing assessment required'; END IF;
      END LOOP;
      IF p_request->>'duration_policy' IS NULL OR p_request->>'duration_policy' NOT IN ('published_ladder','individual_assessment') THEN RAISE EXCEPTION 'Explicit duration assessment required'; END IF;
      IF p_request->>'duration_policy'='published_ladder' AND EXISTS(SELECT 1 FROM moderation_holds h JOIN moderation_cases previous_case ON previous_case.id=h.case_id WHERE h.target_kind=c.target_kind AND h.target_id=c.target_id AND h.case_id<>c.id AND NOT h.rescinded AND NOT h.authority_order AND previous_case.source='legacy_import' AND NOT EXISTS(SELECT 1 FROM moderation_decisions d WHERE d.case_id=h.case_id AND d.actor<>'00000000-0000-0000-0000-000000000000'::uuid AND d.outcome IN ('restrict','uphold') AND d.request->'historical_basis_confirmed'='true'::jsonb)) THEN RAISE EXCEPTION 'Unassessed historic sanctions require individual duration assessment; do not infer their validity'; END IF;
      SELECT count(*) INTO previous_sanctions FROM moderation_holds WHERE target_kind=c.target_kind AND target_id=c.target_id AND case_id<>c.id AND NOT rescinded AND NOT authority_order AND starts_at<=clock_timestamp();
      ladder_days:=CASE previous_sanctions WHEN 0 THEN 3 WHEN 1 THEN 7 WHEN 2 THEN 30 END;
      IF p_request->>'duration_policy'='individual_assessment' AND length(trim(coalesce(p_request->>'duration_reason','')))<3 THEN RAISE EXCEPTION 'Explain the individually assessed duration'; END IF;
      IF c.target_kind='report' THEN
       FOREACH required IN ARRAY ARRAY['prior_warning','absolute_frequency','relative_frequency','seriousness','intent_assessment'] LOOP
        IF length(trim(coalesce(p_request->>required,'')))<1 THEN RAISE EXCEPTION 'Individual warning and misuse assessment required'; END IF;
       END LOOP;
      END IF;
    END IF;
    IF c.target_kind IN ('create','report','account') AND (outcome='restrict' OR (outcome='uphold' AND upheld.effect IS NOT NULL)) AND p_request->>'article23_applicability'='applies' THEN
      FOREACH required IN ARRAY ARRAY['article23_basis','misconduct_facts','proportionality','prior_warning','absolute_frequency','relative_frequency','seriousness','intent_assessment'] LOOP
       IF length(trim(coalesce(p_request->>required,'')))<1 THEN RAISE EXCEPTION 'Applicable Article 23 suspension requires substantiated applicability, prior warning and individual misuse assessment'; END IF;
      END LOOP;
    END IF;
    IF (p_request->>'scope') IS DISTINCT FROM (CASE c.target_kind WHEN 'subtask' THEN 'Diese Teilaufgabe auf Bootstrap Academy' WHEN 'create' THEN 'Erstellen von Teilaufgaben auf Bootstrap Academy' WHEN 'report' THEN 'Melden von Teilaufgaben auf Bootstrap Academy' ELSE 'Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten' END) THEN RAISE EXCEPTION 'Unsupported scope; do not claim an unimplemented restriction'; END IF;
    deadline:=nullif(p_request->>'ends_at','')::timestamptz;
    IF c.target_kind IN ('create','report') AND (outcome='restrict' OR (outcome='uphold' AND upheld.effect IS NOT NULL)) THEN
      IF p_request->>'duration_policy'='published_ladder' THEN
        IF outcome='uphold' AND EXISTS(SELECT 1 FROM moderation_holds WHERE case_id=c.id) THEN SELECT ends_at INTO deadline FROM moderation_holds WHERE case_id=c.id;
        ELSE deadline:=clock_timestamp()+make_interval(days=>ladder_days); END IF;
      END IF;
      IF c.target_kind='report' AND deadline IS NULL AND (p_request->>'article23_applicability' IS DISTINCT FROM 'does_not_apply' OR length(trim(coalesce(p_request->>'article23_basis','')))<3) THEN RAISE EXCEPTION 'A finite report restriction is required unless non-application is actually established'; END IF;
    END IF;
    IF c.target_kind IN ('create','report','account') AND (outcome='restrict' OR (outcome='uphold' AND upheld.effect IS NOT NULL)) AND p_request->>'article23_applicability'='applies' AND deadline IS NULL THEN RAISE EXCEPTION 'Applicable Article 23 suspension requires an assessed finite period'; END IF;
    IF outcome='uphold' THEN
      deadline:=CASE WHEN reviewed.id IS DISTINCT FROM c.latest_decision THEN (reviewed.public_statement->>'ends_at')::timestamptz ELSE upheld.ends_at END;
    END IF;
    historical:=outcome='uphold' AND (reviewed.id IS DISTINCT FROM c.latest_decision OR EXISTS(SELECT 1 FROM moderation_targets WHERE kind=c.target_kind AND id=c.target_id AND withdrawn) OR upheld.ends_at<=clock_timestamp());
    historical:=coalesce(historical,false);
    IF NOT historical AND deadline IS NOT NULL AND deadline<=clock_timestamp() THEN RAISE EXCEPTION 'Restriction end must be in the future'; END IF;
    effect:=CASE outcome WHEN 'remove' THEN 'remove' WHEN 'retire' THEN 'retire' WHEN 'restrict' THEN 'restrict'
      WHEN 'authority_start' THEN CASE WHEN c.target_kind='subtask' THEN 'hide' ELSE 'restrict' END
      WHEN 'authority_change' THEN CASE WHEN c.target_kind='subtask' THEN 'hide' ELSE 'restrict' END
      WHEN 'provisional' THEN 'hide' WHEN 'uphold' THEN
       CASE WHEN reviewed.id IS NOT DISTINCT FROM c.latest_decision THEN upheld.effect
        ELSE CASE reviewed.outcome WHEN 'remove' THEN 'remove' WHEN 'retire' THEN 'retire' WHEN 'provisional' THEN 'hide' WHEN 'restrict' THEN 'restrict' ELSE reviewed.public_statement->>'upheld_measure' END END END;
    IF effect IS NOT NULL AND NOT historical AND outcome<>'uphold' THEN
      IF EXISTS(SELECT 1 FROM moderation_targets WHERE kind=c.target_kind AND id=c.target_id AND withdrawn) AND NOT (outcome='uphold' AND p_request ? 'complaint_id') THEN RAISE EXCEPTION 'Author withdrew target'; END IF;
      INSERT INTO moderation_holds(case_id,target_kind,target_id,effect,starts_at,ends_at,authority_order)
        VALUES(c.id,c.target_kind,c.target_id,effect,clock_timestamp(),deadline,c.source='authority_order')
        ON CONFLICT(case_id) DO UPDATE SET effect=excluded.effect,starts_at=excluded.starts_at,ends_at=excluded.ends_at,active=true,rescinded=false;
    ELSIF outcome IN ('restore','authority_end') THEN
      UPDATE moderation_holds SET active=false,rescinded=NOT (p_actor='00000000-0000-0000-0000-000000000000'::uuid AND p_request->'expired_measure'='true'::jsonb) WHERE case_id=c.id;
    END IF;
    public:=jsonb_build_object('decision_id',d,'case_id',c.id,'target_kind',c.target_kind,'target_id',c.target_id,
      'revision',c.revision+1,'reviewed_content_revision',p_request->'reviewed_content_revision','outcome',outcome,'decided_at',clock_timestamp(),'ends_at',deadline,'notice_available_at',available,
      'rationale',p_request->>'rationale','ground',p_request->>'ground','rule_version',p_request->>'rule_version',
      'automation',p_request->>'automation','scope',p_request->>'scope','redress',p_request->>'redress',
      'article23_applicability',coalesce(p_request->>'article23_applicability','undetermined'),'article23_basis',p_request->>'article23_basis','misconduct_facts',p_request->>'misconduct_facts','proportionality',p_request->>'proportionality','duration_policy',p_request->>'duration_policy','previous_unrescinded_sanctions',previous_sanctions,'hearing',p_request->>'hearing','human_review',p_request->'human_review','review_assessment',p_request->>'review_assessment','historical_only',historical,'reviewed_decision_id',CASE WHEN outcome='uphold' THEN reviewed.id END,'upheld_measure',CASE WHEN outcome='uphold' THEN effect END,
      'effect_before',effect_before,'measure_before',measure_before,
      'measure_after',(SELECT jsonb_build_object('effect',h.effect,'active',h.active,'rescinded',h.rescinded,'ends_at',h.ends_at) FROM moderation_holds h WHERE h.case_id=c.id),
      'effective',moderation_effect(c.target_kind,c.target_id)-'holds');
    INSERT INTO moderation_decisions(id,case_id,actor,request_key,request,outcome,public_statement) VALUES(d,c.id,p_actor,k,p_request,outcome,public);
    UPDATE moderation_cases SET revision=revision+1,latest_decision=d,
      closed_at=NULL,disposition_at=CASE WHEN outcome IN ('restore','authority_end','warn') OR historical OR (outcome='uphold' AND effect IS NULL) THEN clock_timestamp() END,
      work_review_at=CASE WHEN outcome='provisional' THEN clock_timestamp()+interval '7 days' END,notice_review_at=NULL WHERE id=c.id;
    IF historical THEN UPDATE moderation_holds SET active=false WHERE case_id=c.id AND (ends_at<=clock_timestamp() OR EXISTS(SELECT 1 FROM moderation_targets WHERE kind=c.target_kind AND id=c.target_id AND withdrawn)); END IF;
    -- Project after the immutable statement exists, in this same transaction.
    PERFORM moderation_project(c.target_kind,c.target_id);
    INSERT INTO moderation_messages(id,case_id,decision_id,recipient,audience,body,available_at,complaint_until)
      VALUES(gen_random_uuid(),c.id,d,c.subject,'author',public,available,NULL);
    IF c.notifier IS NOT NULL OR c.private_evidence ? 'notifier_contact' THEN
      INSERT INTO moderation_messages(id,case_id,decision_id,recipient,audience,body,available_at,complaint_until)
      VALUES(gen_random_uuid(),c.id,d,c.notifier,'notifier',jsonb_build_object('decision_id',d,'case_id',c.id,'outcome',outcome,
        'text',coalesce(nullif(p_request->>'notifier_rationale',''),p_request->>'rationale'),
        'automation',p_request->>'automation','redress',p_request->>'redress'),available,NULL);
    END IF;
    IF p_request ? 'complaint_id' THEN
      UPDATE moderation_complaints SET outcome_decision=d WHERE id=(p_request->>'complaint_id')::uuid AND case_id=c.id AND outcome_decision IS NULL;
      IF NOT FOUND THEN RAISE EXCEPTION 'Open complaint not found'; END IF;
    END IF;
    PERFORM moderation_update_review_due(c.id);
    RETURN public;
END $$;

-- Historical observations and their initial automatic expiry remain available
-- in the inbox. Only the email transport policy changes; statements stay intact.
-- Only changes with a concrete native before/after comparison are important.
-- Historical/unknown decisions and routine unchanged outcomes stay in the inbox.
CREATE FUNCTION moderation_important_email_decision(p_id uuid) RETURNS boolean LANGUAGE plpgsql STABLE AS $$
DECLARE d moderation_decisions; before_effect jsonb; after_effect jsonb; before_measure jsonb; after_measure jsonb; field text;
BEGIN
 SELECT * INTO d FROM moderation_decisions WHERE id=p_id;
 IF NOT FOUND OR d.outcome NOT IN ('provisional','remove','retire','restrict','restore','authority_start','authority_change','authority_end')
  OR d.public_statement->'historical_only' IS DISTINCT FROM 'false'::jsonb THEN RETURN false; END IF;
 before_effect:=d.public_statement->'effect_before';after_effect:=d.public_statement->'effective';
 before_measure:=d.public_statement->'measure_before';after_measure:=d.public_statement->'measure_after';
 -- Time can expire a different hold even while the target lock is held. A
 -- global difference alone is never evidence that this decision changed access.
 -- Rescinded-only history corrections likewise do not change the own measure.
 IF (before_measure->'active',before_measure->'effect',before_measure->'ends_at')
  IS NOT DISTINCT FROM (after_measure->'active',after_measure->'effect',after_measure->'ends_at') THEN RETURN false; END IF;
 FOREACH field IN ARRAY ARRAY['enabled','removed','retired','withdrawn'] LOOP
  IF jsonb_typeof(before_effect->field) IS DISTINCT FROM 'boolean' OR jsonb_typeof(after_effect->field) IS DISTINCT FROM 'boolean' THEN RETURN false; END IF;
 END LOOP;
 IF (before_effect->'enabled',before_effect->'removed',before_effect->'retired',before_effect->'withdrawn')
  IS DISTINCT FROM (after_effect->'enabled',after_effect->'removed',after_effect->'retired',after_effect->'withdrawn') THEN RETURN true; END IF;
 -- An actual change to an existing measure's effect or duration is important
 -- even when another independent measure currently masks the target state.
 IF before_measure->'active'='true'::jsonb AND after_measure->'active'='true'::jsonb
  AND (before_measure->'effect',before_measure->'ends_at') IS DISTINCT FROM (after_measure->'effect',after_measure->'ends_at') THEN RETURN true; END IF;
 -- Relevant release/expiry of a real measure; repeated restore is not a change.
 IF d.outcome IN ('restore','authority_end') AND before_measure->'active'='true'::jsonb
  AND after_measure->'active'='false'::jsonb AND before_measure->'rescinded'='false'::jsonb
  AND (before_measure->>'ends_at' IS NULL OR (before_measure->>'ends_at')::timestamptz>d.created_at
   OR (d.actor='00000000-0000-0000-0000-000000000000'::uuid AND d.request->'expired_measure'='true'::jsonb)) THEN RETURN true; END IF;
 RETURN false;
END $$;

CREATE FUNCTION moderation_message_email_policy(p_id uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('channel',CASE WHEN basis='important_change' THEN 'email' ELSE 'inbox_only' END,
  'basis',basis,'decision_id',decision_id)
 FROM (
  SELECT m.decision_id,CASE
   WHEN m.body->>'outcome'='legacy_observed' THEN 'legacy_observation'
   WHEN c.source='legacy_import' AND d.actor='00000000-0000-0000-0000-000000000000'::uuid
    AND d.outcome='restore' AND d.request @> '{"expired_measure":true,"expected_revision":1}'::jsonb
    AND EXISTS(SELECT 1 FROM moderation_decisions initial WHERE initial.case_id=c.id
     AND initial.outcome='legacy_observed' AND initial.actor='00000000-0000-0000-0000-000000000000'::uuid
     AND initial.created_at<=d.created_at)
    AND NOT EXISTS(SELECT 1 FROM moderation_decisions previous WHERE previous.case_id=c.id
     AND previous.id<>d.id AND previous.created_at<=d.created_at AND previous.outcome<>'legacy_observed') THEN 'legacy_expiry'
   WHEN m.audience='author' AND m.recipient=c.subject AND m.body=d.public_statement
    AND moderation_important_email_decision(d.id) THEN 'important_change'
   ELSE 'routine_or_unknown'
  END AS basis
  FROM moderation_messages m JOIN moderation_cases c ON c.id=m.case_id
  LEFT JOIN moderation_decisions d ON d.id=m.decision_id AND d.case_id=m.case_id
  WHERE m.id=p_id
 ) policy
$$;

CREATE FUNCTION moderation_email_target_title(p_kind text,p_id uuid) RETURNS text LANGUAGE sql STABLE AS $$
 SELECT NULL::text
$$;

CREATE FUNCTION moderation_message_email_context(p_id uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_strip_nulls(jsonb_build_object('target_kind',c.target_kind,
  'target_title',nullif(moderation_email_target_title(c.target_kind,c.target_id),''),
  'decision_automatic',CASE WHEN d.id IS NOT NULL THEN d.actor='00000000-0000-0000-0000-000000000000'::uuid END))
 FROM moderation_messages m JOIN moderation_cases c ON c.id=m.case_id
 LEFT JOIN moderation_decisions d ON d.id=m.decision_id AND d.case_id=m.case_id WHERE m.id=p_id
$$;

CREATE OR REPLACE FUNCTION moderation_claim(p_limit integer) RETURNS jsonb LANGUAGE sql AS $$
 WITH due AS (SELECT id FROM moderation_messages WHERE relayed_at IS NULL AND available_at<=clock_timestamp()
 AND next_attempt_at<=clock_timestamp() AND (lease_until IS NULL OR lease_until<=clock_timestamp())
 ORDER BY available_at FOR UPDATE SKIP LOCKED LIMIT least(greatest(p_limit,1),50)), claimed AS (
 UPDATE moderation_messages m SET generation=generation+1,attempts=attempts+1,lease_until=clock_timestamp()+interval '2 minutes'
 FROM due WHERE m.id=due.id RETURNING m.*)
 SELECT coalesce(jsonb_agg(jsonb_build_object('id',m.id,'case_id',m.case_id,'recipient',m.recipient,'audience',m.audience,'body',m.body,
 'email_policy',moderation_message_email_policy(m.id),
 'email_context',moderation_message_email_context(m.id),
 'available_at',m.available_at,'complaint_until',m.complaint_until,'generation',m.generation,
 'contact',CASE WHEN m.audience='notifier' THEN c.private_evidence->>'notifier_contact' ELSE c.private_evidence->>'author_contact' END)),'[]')
 FROM claimed m JOIN moderation_cases c ON c.id=m.case_id
$$;

-- Existing append-only transport evidence stores the internal owner policy,
-- without modifying original message bodies, statuses or previous attempts.
CREATE INDEX moderation_delivery_mail_evidence ON moderation_delivery_events(source,message_id)
 WHERE event IN ('email_suppressed','email_approved','owner_email_context');

CREATE FUNCTION moderation_validate_email_policy(p_policy jsonb,p_body jsonb) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 -- Older/missing envelopes may still be adopted, but never grant email authority.
 IF p_policy IS NULL THEN RETURN; END IF;
 IF jsonb_typeof(p_policy) IS DISTINCT FROM 'object'
  OR (p_policy - ARRAY['channel','basis','decision_id']) <> '{}'::jsonb
  OR p_policy->>'channel' IS NULL OR p_policy->>'channel' NOT IN ('email','inbox_only')
  OR p_policy->>'basis' IS NULL
  OR (p_policy->>'channel'='email' AND p_policy->>'basis' NOT IN ('current_message','important_change'))
  OR (p_policy->>'channel'='inbox_only' AND p_policy->>'basis' NOT IN ('legacy_observation','legacy_expiry','routine_or_unknown'))
  OR (p_policy->>'basis' IN ('legacy_observation','legacy_expiry','important_change')
   AND (p_policy->>'decision_id' IS NULL OR p_policy->>'decision_id' IS DISTINCT FROM p_body->>'decision_id'))
  OR (p_policy->>'basis'='legacy_observation' AND p_body->>'outcome' IS DISTINCT FROM 'legacy_observed')
  OR (p_policy->>'basis'='legacy_expiry' AND p_body->>'outcome' IS DISTINCT FROM 'restore') THEN
  RAISE EXCEPTION 'Valid owner email policy required';
 END IF;
END $$;

CREATE FUNCTION moderation_validate_email_context(p_context jsonb) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF p_context IS NULL THEN RETURN; END IF;
 IF jsonb_typeof(p_context) IS DISTINCT FROM 'object'
  OR (p_context - ARRAY['target_kind','target_title','decision_automatic']) <> '{}'::jsonb
  OR p_context->>'target_kind' IS NULL OR p_context->>'target_kind' NOT IN ('subtask','create','report','account')
  OR (p_context ? 'decision_automatic' AND jsonb_typeof(p_context->'decision_automatic') IS DISTINCT FROM 'boolean')
  OR (p_context ? 'target_title' AND (jsonb_typeof(p_context->'target_title') IS DISTINCT FROM 'string'
   OR length(p_context->>'target_title') NOT BETWEEN 1 AND 200
   OR p_context->>'target_title' ~ '[[:cntrl:]]')) THEN
  RAISE EXCEPTION 'Bounded owner email context required';
 END IF;
END $$;

CREATE FUNCTION moderation_record_email_context(p_source text,p_id uuid,p_context jsonb) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF p_context IS NOT NULL AND NOT EXISTS(SELECT 1 FROM moderation_delivery_events
  WHERE source=p_source AND message_id=p_id AND event='owner_email_context') THEN
  INSERT INTO moderation_delivery_events(source,message_id,generation,event,evidence)
   SELECT source,id,generation,'owner_email_context',p_context FROM moderation_delivery WHERE source=p_source AND id=p_id;
 END IF;
 -- Preserve the first accepted context even if a later claim sees a renamed title.
END $$;

CREATE FUNCTION moderation_record_email_policy(p_source text,p_id uuid,p_policy jsonb) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF p_policy->>'channel'='inbox_only' AND NOT EXISTS(SELECT 1 FROM moderation_delivery_events
  WHERE source=p_source AND message_id=p_id AND event='email_suppressed') THEN
  INSERT INTO moderation_delivery_events(source,message_id,generation,event,evidence)
   SELECT source,id,generation,'email_suppressed',p_policy FROM moderation_delivery WHERE source=p_source AND id=p_id;
 ELSIF p_policy->>'channel'='email' AND p_policy->>'basis'='important_change'
  AND NOT EXISTS(SELECT 1 FROM moderation_delivery_events WHERE source=p_source AND message_id=p_id AND event='email_approved') THEN
  INSERT INTO moderation_delivery_events(source,message_id,generation,event,evidence)
   SELECT source,id,generation,'email_approved',p_policy FROM moderation_delivery
   WHERE source=p_source AND id=p_id AND audience='author';
 END IF;
 -- Suppression is permanent for this immutable message, even after a permissive replay.
END $$;

CREATE FUNCTION moderation_email_allowed(p_source text,p_id uuid,p_body jsonb) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT coalesce((SELECT CASE
  -- Only the native explicit recovery flow creates a message without an owner
  -- envelope digest and with a currently valid case-scoped capability.
  WHEN m.source='backend' AND m.audience='recovery' AND m.owner_contact_digest IS NULL THEN
   jsonb_typeof(p_body->'recovery_link')='string' AND jsonb_typeof(p_body->'expires_at')='string'
   AND EXISTS(SELECT 1 FROM moderation_capabilities c WHERE c.hash=p_body->>'capability_hash'
    AND c.subject=m.recipient AND c.case_id=m.case_id AND c.source=p_body->>'case_source'
    AND c.scope='case' AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp())
  WHEN m.audience<>'author' OR p_body->>'outcome'='legacy_observed' THEN false
  WHEN EXISTS(SELECT 1 FROM moderation_delivery_events WHERE source=m.source AND message_id=m.id
   AND event='email_suppressed' AND evidence->>'channel'='inbox_only') THEN false
  WHEN m.source='backend' THEN moderation_message_email_policy(m.id)->>'channel'='email'
  WHEN m.source='challenges' THEN EXISTS(SELECT 1 FROM moderation_delivery_events
   WHERE source=m.source AND message_id=m.id AND event='email_approved'
   AND evidence->>'channel'='email' AND evidence->>'basis'='important_change'
   AND evidence->>'decision_id'=p_body->>'decision_id')
  ELSE false END FROM moderation_delivery m WHERE m.source=p_source AND m.id=p_id),false)
$$;

CREATE OR REPLACE FUNCTION moderation_admit_email(p_source text,p_id uuid,p_generation bigint) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE m moderation_delivery; destination text; attempt uuid:=gen_random_uuid(); authority_source text; authority_audience text;
BEGIN
 SELECT * INTO m FROM moderation_delivery WHERE source=p_source AND id=p_id;
 IF NOT FOUND THEN RETURN 'null'; END IF;
 authority_source:=CASE WHEN m.audience='recovery' THEN m.body->>'case_source' ELSE m.source END;
 PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:'||authority_source||':'||m.case_id,0));
 SELECT * INTO m FROM moderation_delivery WHERE source=p_source AND id=p_id FOR UPDATE;
 IF NOT FOUND OR m.generation IS DISTINCT FROM p_generation OR m.delivered_at IS NOT NULL OR m.status<>'in_flight' OR m.lease_until IS NULL OR m.lease_until<=clock_timestamp()
  OR EXISTS(SELECT 1 FROM moderation_external_disposals WHERE source=authority_source AND case_id=m.case_id)
  OR (authority_source='backend' AND EXISTS(SELECT 1 FROM moderation_disposals WHERE case_id=m.case_id)) THEN RETURN 'null'; END IF;
 -- Recheck policy after the owner-case fence and row re-read, including old claims.
 IF NOT moderation_email_allowed(m.source,m.id,m.body) THEN RETURN 'null'; END IF;
 IF EXISTS(SELECT 1 FROM moderation_send_attempts WHERE source=m.source AND message_id=m.id AND generation=m.generation) THEN RETURN 'null'; END IF;
 authority_audience:=CASE WHEN m.audience='recovery' THEN m.body->>'recipient_audience' ELSE m.audience END;
 SELECT c.contact INTO destination FROM moderation_recipient_contacts c WHERE (c.source,c.case_id,c.recipient,c.audience)=(authority_source,m.case_id,m.recipient,authority_audience)
  AND c.authority_kind IN ('verified_account','verified_case','notifier_channel');
 IF m.audience='recovery' AND (destination IS DISTINCT FROM m.contact OR (m.body->>'expires_at')::timestamptz<=clock_timestamp()
  OR NOT EXISTS(SELECT 1 FROM moderation_capabilities c WHERE c.hash=m.body->>'capability_hash' AND c.subject=m.recipient AND c.case_id=m.case_id AND c.source=authority_source AND c.scope='case' AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp())) THEN
  UPDATE moderation_delivery SET status='expired',lease_until=NULL,body=body-'recovery_link' WHERE source=m.source AND id=m.id;
  RETURN 'null';
 END IF;
 IF destination IS NULL THEN
  UPDATE moderation_delivery SET status='no_contact',lease_until=NULL WHERE source=m.source AND id=m.id;
  INSERT INTO moderation_delivery_events(source,message_id,generation,event,evidence) VALUES(m.source,m.id,m.generation,'no_contact','{"meaning":"No currently established recipient authority; human recovery remains necessary."}');
  RETURN 'null';
 END IF;
 UPDATE moderation_delivery SET lease_until=clock_timestamp()+interval '60 seconds' WHERE source=m.source AND id=m.id;
 INSERT INTO moderation_send_attempts(id,source,authority_source,message_id,case_id,generation,lease_until,destination_digest,body_digest)
  VALUES(attempt,m.source,authority_source,m.id,m.case_id,m.generation,clock_timestamp()+interval '60 seconds',encode(sha256(convert_to(jsonb_build_array(destination)::text,'UTF8')),'hex'),encode(sha256(convert_to(m.body::text,'UTF8')),'hex'));
 INSERT INTO moderation_delivery_events(source,message_id,generation,event,evidence) VALUES(m.source,m.id,m.generation,'send_admitted',jsonb_build_object('attempt_id',attempt,'meaning','A bounded attempt was authorized. This is not proof of transmission or recipient information.'));
 RETURN to_jsonb(m)||jsonb_build_object('contact',destination,'attempt_id',attempt,
  'recipient_name',(SELECT name FROM users WHERE id=m.recipient AND email_verified AND email=destination),
  'email_context',(SELECT evidence FROM moderation_delivery_events WHERE source=m.source AND message_id=m.id AND event='owner_email_context' ORDER BY id LIMIT 1));
END $$;

-- The current public dispatcher wraps this function with invoice-identity
-- protections (2026-09-10). Preserve that outer wrapper without replacement.
CREATE OR REPLACE FUNCTION backend_moderation_before_invoice_identity(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE target_subject uuid; source_name text; contact_minimized boolean; old moderation_delivery; cap moderation_capabilities; claimed_record moderation_delivery; claimed_messages jsonb:='[]'; opened_case uuid;
BEGIN
 CASE p_operation
 WHEN 'retained_records' THEN
  RETURN jsonb_build_object(
   'complete',EXISTS(SELECT 1 FROM users WHERE id=p_actor) OR EXISTS(SELECT 1 FROM moderation_erasure_events WHERE subject=p_actor AND retained_owner_inventory),
   'scope_note',CASE WHEN EXISTS(SELECT 1 FROM users WHERE id=p_actor) OR EXISTS(SELECT 1 FROM moderation_erasure_events WHERE subject=p_actor AND retained_owner_inventory)
    THEN 'Live owner references and preserved pre-erasure ownership identify surviving records. Erased account/profile fields are not reconstructed; archived documents remain available through their owned download routes.'
    ELSE 'Earlier erasure has no complete retained-owner inventory. Surviving directly owner-bound records are included; unlinked historical declarations/documents require proportionate human verification. Completeness is not established.' END,
   'paypal_payments',coalesce((SELECT jsonb_agg(to_jsonb(p)-'last_error') FROM paypal_payments p WHERE p.user_id=p_actor),'[]'::jsonb),
   'financial_documents',coalesce((SELECT jsonb_agg(to_jsonb(d)) FROM financial_documents d WHERE d.user_id=p_actor OR EXISTS(SELECT 1 FROM moderation_retained_record_owners o WHERE o.subject=p_actor AND o.kind='financial_document' AND o.record_id=d.number) OR EXISTS(SELECT 1 FROM paypal_payments p WHERE p.user_id=p_actor AND d.number='R'||lpad(p.invoice_number::text,7,'0'))),'[]'::jsonb),
   'original_invoices',coalesce((SELECT jsonb_agg(jsonb_build_object('invoice_number',i.invoice_number,'pdf_base64',encode(i.pdf,'base64'),'provenance',i.provenance,'recorded_at',i.recorded_at)) FROM invoice_originals i WHERE EXISTS(SELECT 1 FROM financial_documents d WHERE d.number=i.invoice_number AND (d.user_id=p_actor OR EXISTS(SELECT 1 FROM moderation_retained_record_owners o WHERE o.subject=p_actor AND o.kind='financial_document' AND o.record_id=d.number))) OR EXISTS(SELECT 1 FROM paypal_payments p WHERE p.user_id=p_actor AND i.invoice_number='R'||lpad(p.invoice_number::text,7,'0'))),'[]'::jsonb),
   'contract_declarations',coalesce((SELECT jsonb_agg(to_jsonb(d)) FROM contract_declarations d WHERE d.user_id=p_actor OR EXISTS(SELECT 1 FROM moderation_retained_record_owners o WHERE o.subject=p_actor AND o.kind='contract_declaration' AND o.record_id=d.id::text)),'[]'::jsonb),
   'paypal_legacy_reconciliation',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM paypal_legacy_reconciliation r WHERE r.user_id=p_actor),'[]'::jsonb),
   'premium_period_changes',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM premium_period_changes r WHERE r.user_id=p_actor),'[]'::jsonb),
   'internal_coin_operations',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM internal_coin_operations r WHERE r.user_id=p_actor),'[]'::jsonb),
   'contract_evidence',coalesce((SELECT jsonb_agg(jsonb_build_object('declaration',to_jsonb(d),'contract_delivery',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_delivery r WHERE r.declaration_id=d.id),'[]'::jsonb),'contract_cancellation_schedule',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_cancellation_schedule r WHERE r.declaration_id=d.id),'[]'::jsonb),'contract_account_observation',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_account_observation r WHERE r.declaration_id=d.id),'[]'::jsonb),'contract_period_observation',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_period_observation r WHERE r.declaration_id=d.id),'[]'::jsonb),'contract_premium_operations',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_premium_operations r WHERE r.declaration_id=d.id),'[]'::jsonb),'contract_processing_actions',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_processing_actions r WHERE r.declaration_id=d.id),'[]'::jsonb),'contract_purchase_observations',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_purchase_observations r WHERE r.declaration_id=d.id),'[]'::jsonb))) FROM contract_declarations d WHERE d.user_id=p_actor OR EXISTS(SELECT 1 FROM moderation_retained_record_owners o WHERE o.subject=p_actor AND o.kind='contract_declaration' AND o.record_id=d.id::text) OR EXISTS(SELECT 1 FROM contract_account_observation a WHERE a.declaration_id=d.id AND a.user_id=p_actor)),'[]'::jsonb),
   'paypal_receipt_artifacts',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM paypal_receipt_artifacts r WHERE EXISTS(SELECT 1 FROM paypal_payments p WHERE p.order_id=r.order_id AND p.user_id=p_actor)),'[]'::jsonb),
   'paypal_receipt_observations',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM paypal_receipt_observations r WHERE EXISTS(SELECT 1 FROM paypal_payments p WHERE p.order_id=r.order_id AND p.user_id=p_actor)),'[]'::jsonb),
   'withdrawal_consents',coalesce((SELECT jsonb_agg(to_jsonb(w)) FROM withdrawal_consents w WHERE w.user_id=p_actor),'[]'::jsonb));
 WHEN 'rule_evidence' THEN
  RETURN coalesce((SELECT jsonb_build_object('recorded_terms_version',terms_version,'recorded_acceptance_at',terms_accepted_at,
   'contact',email,'contract_validity','not_assessed','automatic_quality_basis_confirmed',false,
   'explanation','A current account field alone does not establish the applicable original document, historical assent or legal capacity. Human basis review is required.') FROM users WHERE id=p_actor),
   jsonb_build_object('recorded_acceptance','unknown','automatic_quality_basis_confirmed',false));
 WHEN 'open' THEN
  target_subject:=(p_body->>'target_id')::uuid;
  PERFORM 1 FROM users WHERE id=target_subject FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'Target unavailable'; END IF;
  PERFORM pg_advisory_xact_lock(hashtextextended('moderation:account:'||target_subject,0));
  IF p_body->>'source' NOT IN ('own_review','email_notice','authority_order') THEN RAISE EXCEPTION 'Unsupported source'; END IF;
  opened_case:=moderation_open((p_body->>'id')::uuid,p_actor,'account',target_subject,target_subject,p_body->>'source',(p_body->>'notifier')::uuid,coalesce(p_body->'private_evidence','{}')||jsonb_build_object('author_contact',(SELECT email FROM users WHERE id=target_subject),'author_contact_verified',(SELECT email_verified FROM users WHERE id=target_subject),'recorded_terms_version',(SELECT terms_version FROM users WHERE id=target_subject),'recorded_acceptance_at',(SELECT terms_accepted_at FROM users WHERE id=target_subject),'contract_validity','not_assessed'));
  INSERT INTO moderation_recipient_contacts(source,case_id,recipient,audience,contact,basis,authority_kind)
   SELECT 'backend',opened_case,target_subject,'author',CASE WHEN email_verified THEN email END,'account_contact_at_case_open',CASE WHEN email_verified THEN 'verified_account' ELSE 'unknown' END FROM users WHERE id=target_subject ON CONFLICT DO NOTHING;
  RETURN to_jsonb(opened_case);
 WHEN 'decide' THEN RETURN moderation_decide(p_actor,p_body);
 WHEN 'queue' THEN RETURN moderation_queue(coalesce((p_body->>'limit')::integer,50),coalesce((p_body->>'offset')::integer,0));
 WHEN 'case' THEN RETURN coalesce((SELECT to_jsonb(c)||jsonb_build_object('decisions',(SELECT coalesce(jsonb_agg(to_jsonb(d) ORDER BY d.created_at),'[]') FROM moderation_decisions d WHERE d.case_id=c.id),'complaints',(SELECT coalesce(jsonb_agg(to_jsonb(a)),'[]') FROM moderation_complaints a WHERE a.case_id=c.id),'escalations',(SELECT coalesce(jsonb_agg(to_jsonb(e)),'[]') FROM moderation_escalations e WHERE e.case_id=c.id),'handling',(SELECT coalesce(jsonb_agg(to_jsonb(h)),'[]') FROM moderation_handling h WHERE h.case_id=c.id),'handling_events',(SELECT coalesce(jsonb_agg(to_jsonb(h) ORDER BY h.recorded_at),'[]') FROM moderation_handling_events h WHERE h.case_id=c.id),'retention_reviews',(SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY r.recorded_at),'[]') FROM moderation_retention_reviews r WHERE r.case_id=c.id),'effective',moderation_effect(c.target_kind,c.target_id)) FROM moderation_cases c WHERE c.id=(p_body->>'id')::uuid),'null');
 WHEN 'handling' THEN
  IF length(coalesce(p_body->>'facts',''))<3 OR length(coalesce(p_body->>'status',''))<3 THEN RAISE EXCEPTION 'Actual handling facts required'; END IF;
  PERFORM 1 FROM moderation_handling_events WHERE id=(p_body->>'id')::uuid;
  IF FOUND THEN IF NOT EXISTS(SELECT 1 FROM moderation_handling_events WHERE id=(p_body->>'id')::uuid AND actor=p_actor AND record=p_body) THEN RAISE EXCEPTION 'Conflicting handling replay'; END IF; RETURN 'true'; END IF;
  INSERT INTO moderation_handling_events(id,case_id,actor,record) VALUES((p_body->>'id')::uuid,(p_body->>'case_id')::uuid,p_actor,p_body);
  UPDATE moderation_handling SET status=p_body->>'status',record=p_body||jsonb_build_object('actor',p_actor,'recorded_at',clock_timestamp()) WHERE case_id=(p_body->>'case_id')::uuid AND kind=p_body->>'kind';
  IF NOT FOUND THEN RAISE EXCEPTION 'Handling work unavailable'; END IF;
  PERFORM moderation_update_review_due((p_body->>'case_id')::uuid);
  RETURN 'true';
 WHEN 'maintenance' THEN RETURN to_jsonb(moderation_maintenance());
 WHEN 'retention' THEN RETURN to_jsonb(moderation_retention(p_actor,p_body));
 WHEN 'escalate' THEN RETURN to_jsonb(moderation_escalate(p_actor,p_body));
 WHEN 'inbox' THEN RETURN moderation_inbox(p_actor);
 WHEN 'opened' THEN RETURN to_jsonb(moderation_opened(p_actor,(p_body->>'id')::uuid));
 WHEN 'complain' THEN RETURN to_jsonb(moderation_complain(p_actor,p_body));
 WHEN 'issue_capability' THEN
  INSERT INTO moderation_capabilities(hash,subject,case_id,scope,expires_at) VALUES(p_body->>'hash',p_actor,(p_body->>'case_id')::uuid,p_body->>'scope',clock_timestamp()+interval '30 minutes');
  RETURN jsonb_build_object('expires_at',clock_timestamp()+interval '30 minutes');
 WHEN 'capability' THEN
  SELECT * INTO cap FROM moderation_capabilities WHERE hash=p_body->>'hash' AND revoked_at IS NULL AND expires_at>clock_timestamp();
  RETURN CASE WHEN FOUND THEN jsonb_build_object('subject',cap.subject,'case_id',cap.case_id,'scope',cap.scope,'source',cap.source) ELSE 'null' END;
 WHEN 'revoke_capability' THEN
  UPDATE moderation_capabilities SET revoked_at=clock_timestamp() WHERE hash=p_body->>'hash';
  RETURN 'true';
 WHEN 'recovery_request' THEN
  PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:'||(p_body->>'source')||':'||(p_body->>'case_id'),0));
  IF EXISTS(SELECT 1 FROM moderation_external_disposals WHERE source=p_body->>'source' AND case_id=(p_body->>'case_id')::uuid) OR (p_body->>'source'='backend' AND EXISTS(SELECT 1 FROM moderation_disposals WHERE case_id=(p_body->>'case_id')::uuid)) THEN RETURN 'true'; END IF;
  -- The response is identical for absent cases, wrong addresses and throttling.
  -- Only an already-recorded case contact can receive this case-scoped proof.
  PERFORM pg_advisory_xact_lock(hashtextextended('moderation-recovery:'||(p_body->>'ip_hash'),0));
  DELETE FROM moderation_recovery_attempts WHERE requested_at<clock_timestamp()-interval '1 day';
  IF (SELECT count(*) FROM moderation_recovery_attempts WHERE ip_hash=p_body->>'ip_hash' AND requested_at>clock_timestamp()-interval '1 hour')>=10 THEN RETURN 'true'; END IF;
  INSERT INTO moderation_recovery_attempts(ip_hash) VALUES(p_body->>'ip_hash');
  SELECT m.* INTO old FROM moderation_recipient_contacts c JOIN moderation_delivery m ON (m.source,m.case_id,m.recipient,m.audience)=(c.source,c.case_id,c.recipient,c.audience) WHERE c.source=p_body->>'source' AND c.case_id=(p_body->>'case_id')::uuid AND lower(trim(c.contact))=lower(trim(p_body->>'contact')) AND c.audience IN ('author','notifier') AND c.authority_kind IN ('verified_account','verified_case','notifier_channel') ORDER BY m.accepted_at DESC LIMIT 1;
  IF NOT FOUND THEN RETURN 'true'; END IF;
  old.contact:=(SELECT contact FROM moderation_recipient_contacts WHERE (source,case_id,recipient,audience)=(old.source,old.case_id,old.recipient,old.audience));
  IF EXISTS(SELECT 1 FROM moderation_capabilities WHERE subject=old.recipient AND case_id=old.case_id AND source=old.source AND revoked_at IS NULL AND expires_at>clock_timestamp() AND created_at>clock_timestamp()-interval '5 minutes') THEN RETURN 'true'; END IF;
  INSERT INTO moderation_capabilities(hash,subject,case_id,scope,source,expires_at) VALUES(p_body->>'hash',old.recipient,old.case_id,'case',old.source,clock_timestamp()+interval '30 minutes');
  INSERT INTO moderation_delivery(source,id,case_id,recipient,audience,body,contact,owner_available_at)
   VALUES('backend',gen_random_uuid(),old.case_id,old.recipient,'recovery',jsonb_build_object('recovery_link',p_body->>'link','text','Ein Zugang zu diesem Vorgang wurde angefordert. Der Link gilt 30 Minuten und berechtigt nur zum Lesen dieses Vorgangs und zu einer Beschwerde. Falls du ihn nicht angefordert hast, musst du nichts tun.','expires_at',clock_timestamp()+interval '30 minutes','case_source',old.source,'recipient_audience',old.audience,'capability_hash',p_body->>'hash'),old.contact,clock_timestamp());
  RETURN 'true';
 WHEN 'delivery_queue' THEN RETURN coalesce((SELECT jsonb_agg(to_jsonb(m)-'body'||jsonb_build_object('current_contact',(SELECT contact FROM moderation_recipient_contacts c WHERE (c.source,c.case_id,c.recipient,c.audience)=(m.source,m.case_id,m.recipient,m.audience)),'body',CASE WHEN audience='recovery' THEN jsonb_build_object('text','Case access link; secret omitted') ELSE body END)) FROM (SELECT * FROM moderation_delivery WHERE ((p_body->>'case_id') IS NULL AND status<>'transport_accepted') OR (case_id=(p_body->>'case_id')::uuid AND source=p_body->>'source') ORDER BY next_attempt_at LIMIT 100) m),'[]');
 WHEN 'delivery_contact' THEN
  IF length(trim(coalesce(p_body->>'verification_evidence','')))<3 OR length(trim(coalesce(p_body->>'contact','')))<3 THEN RAISE EXCEPTION 'Verified contact and actual proof required'; END IF;
  SELECT * INTO old FROM moderation_delivery WHERE source=p_body->>'source' AND id=(p_body->>'id')::uuid AND audience<>'recovery';
  IF NOT FOUND THEN RAISE EXCEPTION 'Recipient message unavailable'; END IF;
  PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:'||old.source||':'||old.case_id,0));
  SELECT * INTO old FROM moderation_delivery WHERE source=p_body->>'source' AND id=(p_body->>'id')::uuid AND audience<>'recovery' FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'Recipient message unavailable'; END IF;
  INSERT INTO moderation_delivery_events(source,message_id,generation,event,evidence) VALUES(old.source,old.id,old.generation,'verified_contact_change',jsonb_build_object('actor',p_actor,'verification_evidence',p_body->>'verification_evidence','old_contact_digest',encode(sha256(convert_to(jsonb_build_array(old.contact)::text,'UTF8')),'hex'),'contact_digest',encode(sha256(convert_to(jsonb_build_array(p_body->>'contact')::text,'UTF8')),'hex')));
  UPDATE moderation_recipient_contacts SET contact=p_body->>'contact',updated_at=clock_timestamp(),basis='verified_case_contact_correction',authority_kind='verified_case' WHERE (source,case_id,recipient,audience)=(old.source,old.case_id,old.recipient,old.audience);
  UPDATE moderation_capabilities SET revoked_at=clock_timestamp() WHERE subject=old.recipient AND case_id=old.case_id AND source=old.source AND scope='case' AND revoked_at IS NULL;
  -- Preserve original addresses and transport bytes. This invalidates only
  -- uncompleted leases and changes the next claim's explicit destination.
  UPDATE moderation_delivery SET status='retry',next_attempt_at=clock_timestamp(),lease_until=NULL,generation=generation+1 WHERE (source,case_id,recipient,audience)=(old.source,old.case_id,old.recipient,old.audience) AND delivered_at IS NULL;
  UPDATE moderation_delivery SET status='expired',body=body-'recovery_link',lease_until=NULL,generation=generation+1 WHERE audience='recovery' AND body->>'case_source'=old.source AND case_id=old.case_id AND recipient=old.recipient;
  RETURN jsonb_build_object('corrected',true,'in_progress_attempts',(SELECT count(*) FROM moderation_send_attempts a WHERE a.case_id=old.case_id AND a.authority_source=old.source AND a.lease_until>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM moderation_send_outcomes o WHERE o.attempt_id=a.id)),
   'transport_boundary','Queued attempts use the current contact. A previously admitted bounded attempt may already be in progress; correction does not recall transmission.');
 WHEN 'claim' THEN RETURN moderation_claim(25);
 WHEN 'ack' THEN RETURN to_jsonb(moderation_ack((p_body->>'id')::uuid,(p_body->>'generation')::bigint,(p_body->>'ok')::boolean));
 WHEN 'accept_minimization' THEN
  IF p_body->>'source' IS DISTINCT FROM 'challenges' OR p_body->>'field' IS NULL OR p_body->>'field' NOT IN ('author_contact','notifier_contact') OR p_body->>'id' IS NULL THEN RAISE EXCEPTION 'Unsupported contact minimization'; END IF;
  PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:challenges:'||(p_body->>'case_id'),0));
  IF EXISTS(SELECT 1 FROM moderation_external_minimizations WHERE id=(p_body->>'id')::uuid) THEN
   IF NOT EXISTS(SELECT 1 FROM moderation_external_minimizations WHERE id=(p_body->>'id')::uuid AND case_id=(p_body->>'case_id')::uuid AND field=p_body->>'field') THEN RAISE EXCEPTION 'Conflicting minimization replay'; END IF; RETURN 'true';
  END IF;
  INSERT INTO moderation_external_minimizations(id,case_id,field) VALUES((p_body->>'id')::uuid,(p_body->>'case_id')::uuid,p_body->>'field');
  PERFORM moderation_minimize_contacts('challenges',(p_body->>'case_id')::uuid,CASE p_body->>'field' WHEN 'author_contact' THEN 'author' ELSE 'notifier' END);
  RETURN 'true';
 WHEN 'accept_disposal' THEN
  IF p_body->>'source' IS DISTINCT FROM 'challenges' THEN RAISE EXCEPTION 'Unsupported disposal owner'; END IF;
  PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:challenges:'||(p_body->>'case_id'),0));
  INSERT INTO moderation_external_disposals(source,case_id,review_due_at) VALUES('challenges',(p_body->>'case_id')::uuid,coalesce((p_body->>'review_due_at')::timestamptz,clock_timestamp())) ON CONFLICT DO NOTHING;
  PERFORM moderation_dispose_delivery('challenges',(p_body->>'case_id')::uuid);
  RETURN 'true';
 WHEN 'accept_delivery' THEN
  source_name:=p_body->>'source';
  -- Coherently register first author authority against email correction and
  -- erasure. All account mutations acquire users before recipient-case locks.
  IF p_body->>'audience'='author' AND p_body->>'recipient' IS NOT NULL THEN
   PERFORM 1 FROM users WHERE id=(p_body->>'recipient')::uuid FOR UPDATE;
  END IF;
  PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:'||source_name||':'||(p_body->>'case_id'),0));
  IF EXISTS(SELECT 1 FROM moderation_external_disposals WHERE source=source_name AND case_id=(p_body->>'case_id')::uuid) OR (source_name='backend' AND EXISTS(SELECT 1 FROM moderation_disposals WHERE case_id=(p_body->>'case_id')::uuid)) THEN RETURN 'true'; END IF;
  IF source_name IS NULL OR source_name NOT IN ('backend','challenges') OR jsonb_typeof(p_body->'body') IS DISTINCT FROM 'object' OR (p_body->>'id') IS NULL OR (p_body->>'case_id') IS NULL OR (p_body->>'available_at') IS NULL OR p_body->>'audience' IS NULL THEN RAISE EXCEPTION 'Complete owner message required'; END IF;
  PERFORM moderation_validate_email_policy(p_body->'email_policy',p_body->'body');
  PERFORM moderation_validate_email_context(p_body->'email_context');
  PERFORM pg_advisory_xact_lock(hashtextextended('delivery:'||source_name||':'||(p_body->>'id'),0));
  SELECT * INTO old FROM moderation_delivery WHERE moderation_delivery.source=source_name AND id=(p_body->>'id')::uuid;
  IF FOUND THEN
   IF old.case_id<>(p_body->>'case_id')::uuid OR old.recipient IS DISTINCT FROM (p_body->>'recipient')::uuid OR old.audience<>p_body->>'audience' OR old.body IS DISTINCT FROM p_body->'body' OR old.owner_available_at IS DISTINCT FROM (p_body->>'available_at')::timestamptz OR old.owner_contact_digest IS DISTINCT FROM encode(sha256(convert_to(jsonb_build_array(p_body->'contact')::text,'UTF8')),'hex') THEN RAISE EXCEPTION 'Conflicting delivery replay'; END IF;
   PERFORM moderation_record_email_policy(source_name,old.id,p_body->'email_policy');
   PERFORM moderation_record_email_context(source_name,old.id,p_body->'email_context');
   RETURN 'true';
  END IF;
  -- A reviewed minimization also fences a not-yet-adopted owner claim. Keep
  -- only its digest for exact replay; a later verified correction is explicit.
  contact_minimized:=coalesce((SELECT basis='reviewed_contact_minimization' FROM moderation_recipient_contacts WHERE source=source_name AND case_id=(p_body->>'case_id')::uuid AND recipient=(p_body->>'recipient')::uuid AND audience=p_body->>'audience'),
    EXISTS(SELECT 1 FROM moderation_external_minimizations WHERE source_name='challenges' AND case_id=(p_body->>'case_id')::uuid AND field=CASE p_body->>'audience' WHEN 'author' THEN 'author_contact' WHEN 'notifier' THEN 'notifier_contact' END)
    OR EXISTS(SELECT 1 FROM moderation_retention_reviews WHERE source_name='backend' AND case_id=(p_body->>'case_id')::uuid AND record->>'action'='minimize' AND record->'unnecessary_fields' ? CASE p_body->>'audience' WHEN 'author' THEN 'author_contact' WHEN 'notifier' THEN 'notifier_contact' END));
  INSERT INTO moderation_delivery(source,id,case_id,recipient,audience,body,contact,owner_contact,owner_contact_digest,owner_available_at)
   VALUES(source_name,(p_body->>'id')::uuid,(p_body->>'case_id')::uuid,(p_body->>'recipient')::uuid,p_body->>'audience',p_body->'body',CASE WHEN contact_minimized THEN NULL WHEN p_body->>'audience'='author' THEN (SELECT email FROM users WHERE id=(p_body->>'recipient')::uuid AND email_verified) ELSE p_body->>'contact' END,CASE WHEN contact_minimized THEN NULL ELSE p_body->>'contact' END,encode(sha256(convert_to(jsonb_build_array(p_body->'contact')::text,'UTF8')),'hex'),(p_body->>'available_at')::timestamptz);
  INSERT INTO moderation_recipient_contacts(source,case_id,recipient,audience,contact,basis,authority_kind)
   SELECT source_name,(p_body->>'case_id')::uuid,(p_body->>'recipient')::uuid,p_body->>'audience',(SELECT contact FROM moderation_delivery WHERE source=source_name AND id=(p_body->>'id')::uuid),CASE WHEN contact_minimized THEN 'reviewed_contact_minimization' WHEN p_body->>'audience'='author' THEN 'current_verified_account_or_unknown' ELSE 'notifier_supplied_channel' END,CASE WHEN contact_minimized THEN 'minimized' WHEN p_body->>'audience'='notifier' THEN 'notifier_channel' WHEN EXISTS(SELECT 1 FROM users WHERE id=(p_body->>'recipient')::uuid AND email_verified) THEN 'verified_account' ELSE 'unknown' END
   WHERE p_body->>'recipient' IS NOT NULL
   ON CONFLICT DO NOTHING;
  PERFORM moderation_record_email_policy(source_name,(p_body->>'id')::uuid,p_body->'email_policy');
  PERFORM moderation_record_email_context(source_name,(p_body->>'id')::uuid,p_body->'email_context');
  RETURN 'true';
 WHEN 'claim_email' THEN
  UPDATE moderation_delivery SET body=body-'recovery_link',status=CASE WHEN delivered_at IS NULL THEN 'expired' ELSE status END,lease_until=NULL WHERE audience='recovery' AND body ? 'recovery_link' AND (body->>'expires_at')::timestamptz<=clock_timestamp();
  FOR claimed_record IN SELECT * FROM moderation_delivery WHERE delivered_at IS NULL AND moderation_email_allowed(source,id,body) AND status NOT IN ('no_contact','expired') AND next_attempt_at<=clock_timestamp() AND (lease_until IS NULL OR lease_until<=clock_timestamp()) ORDER BY accepted_at FOR UPDATE SKIP LOCKED LIMIT 25 LOOP
   IF claimed_record.lease_until IS NOT NULL THEN
    INSERT INTO moderation_delivery_events(source,message_id,generation,event,evidence) VALUES(claimed_record.source,claimed_record.id,claimed_record.generation,'lease_expired_uncertain','{"meaning":"The previous worker did not confirm an outcome; transport may have accepted the message. A retry may duplicate it."}');
   END IF;
   UPDATE moderation_delivery SET generation=generation+1,attempts=attempts+1,lease_until=clock_timestamp()+interval '2 minutes',status='in_flight' WHERE source=claimed_record.source AND id=claimed_record.id;
   INSERT INTO moderation_delivery_events(source,message_id,generation,event,evidence) VALUES(claimed_record.source,claimed_record.id,claimed_record.generation+1,'claimed',jsonb_build_object('destination_digest',encode(sha256(convert_to(jsonb_build_array(CASE WHEN claimed_record.audience='recovery' THEN claimed_record.contact ELSE (SELECT c.contact FROM moderation_recipient_contacts c WHERE (c.source,c.case_id,c.recipient,c.audience)=(claimed_record.source,claimed_record.case_id,claimed_record.recipient,claimed_record.audience)) END)::text,'UTF8')),'hex')));
   claimed_messages:=claimed_messages||(SELECT jsonb_build_array(jsonb_build_object('source',m.source,'id',m.id,'generation',m.generation)) FROM moderation_delivery m WHERE m.source=claimed_record.source AND m.id=claimed_record.id);
  END LOOP;
  RETURN claimed_messages;
 WHEN 'admit_email' THEN RETURN moderation_admit_email(p_body->>'source',(p_body->>'id')::uuid,(p_body->>'generation')::bigint);
 WHEN 'ack_email' THEN RETURN to_jsonb(moderation_ack_email(p_body));
 ELSE RAISE EXCEPTION 'Unsupported moderation operation';
 END CASE;
END $$;
