-- Each service owns its restrictions. No live-account/content foreign keys:
-- erasure and appeal evidence have deliberately separate lifecycles.
CREATE TABLE moderation_targets (
    kind text NOT NULL CHECK(kind IN ('subtask','create','report','account')),
    id uuid NOT NULL, subject uuid NOT NULL, withdrawn boolean NOT NULL DEFAULT false,
    content_revision bigint NOT NULL DEFAULT 0,
    base_enabled boolean NOT NULL DEFAULT true, base_retired boolean NOT NULL DEFAULT false,
    PRIMARY KEY(kind,id)
);
CREATE TABLE moderation_cases (
    id uuid PRIMARY KEY, target_kind text NOT NULL, target_id uuid NOT NULL, subject uuid NOT NULL,
    source text NOT NULL CHECK(source IN ('user_report','rating_threshold','email_notice','own_review','legacy_import','authority_order')),
    received_at timestamptz NOT NULL DEFAULT clock_timestamp(), created_by uuid,
    notifier uuid, private_evidence jsonb NOT NULL, revision integer NOT NULL DEFAULT 0,
    latest_decision uuid, notice_after timestamptz, review_due_at timestamptz, disposition_at timestamptz, closed_at timestamptz,
    FOREIGN KEY(target_kind,target_id) REFERENCES moderation_targets(kind,id)
);
CREATE TABLE moderation_decisions (
    id uuid PRIMARY KEY, case_id uuid NOT NULL REFERENCES moderation_cases(id),
    actor uuid NOT NULL, request_key uuid NOT NULL, request jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(), outcome text NOT NULL,
    public_statement jsonb NOT NULL, UNIQUE(actor,request_key)
);
CREATE TABLE moderation_holds (
    case_id uuid PRIMARY KEY REFERENCES moderation_cases(id), target_kind text NOT NULL, target_id uuid NOT NULL,
    effect text NOT NULL CHECK(effect IN ('hide','remove','retire','restrict')),
    starts_at timestamptz NOT NULL, ends_at timestamptz, active boolean NOT NULL DEFAULT true, rescinded boolean NOT NULL DEFAULT false,
    authority_order boolean NOT NULL DEFAULT false,
    FOREIGN KEY(target_kind,target_id) REFERENCES moderation_targets(kind,id)
);
CREATE INDEX moderation_active_targets ON moderation_holds(target_kind,target_id) WHERE active;
CREATE TABLE moderation_messages (
    id uuid PRIMARY KEY, case_id uuid NOT NULL REFERENCES moderation_cases(id), decision_id uuid REFERENCES moderation_decisions(id),
    recipient uuid, audience text NOT NULL CHECK(audience IN ('author','notifier')),
    body jsonb NOT NULL, available_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    first_opened_at timestamptz, informed_at timestamptz, notification_evidence jsonb, complaint_until timestamptz,
    relayed_at timestamptz, attempts integer NOT NULL DEFAULT 0, generation bigint NOT NULL DEFAULT 0,
    next_attempt_at timestamptz NOT NULL DEFAULT clock_timestamp(), lease_until timestamptz,
    delivery_status text NOT NULL DEFAULT 'pending', delivery_error text,
    UNIQUE(decision_id,audience)
);
CREATE INDEX moderation_due_messages ON moderation_messages(next_attempt_at) WHERE relayed_at IS NULL;
CREATE TABLE moderation_complaints (
    id uuid PRIMARY KEY, case_id uuid NOT NULL REFERENCES moderation_cases(id), decision_id uuid NOT NULL REFERENCES moderation_decisions(id),
    complainant uuid NOT NULL, text text NOT NULL CHECK(length(text) BETWEEN 1 AND 16000),
    received_at timestamptz NOT NULL DEFAULT clock_timestamp(), outcome_decision uuid REFERENCES moderation_decisions(id),
    UNIQUE(complainant,id)
);
CREATE TABLE moderation_escalations (
    id uuid PRIMARY KEY, case_id uuid NOT NULL REFERENCES moderation_cases(id), actor uuid NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(), record jsonb NOT NULL
);
CREATE TABLE moderation_erasure_events (
    subject uuid PRIMARY KEY, erased_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE FUNCTION moderation_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='UPDATE' OR current_setting('academy.moderation_disposal',true) IS DISTINCT FROM 'authorized' THEN
        RAISE EXCEPTION 'Moderation evidence is immutable; append a correction or use reviewed disposal';
    END IF;
    RETURN OLD;
END $$;
CREATE TRIGGER moderation_decisions_immutable BEFORE UPDATE OR DELETE ON moderation_decisions FOR EACH ROW EXECUTE FUNCTION moderation_immutable();
CREATE TRIGGER moderation_escalations_immutable BEFORE UPDATE OR DELETE ON moderation_escalations FOR EACH ROW EXECUTE FUNCTION moderation_immutable();

CREATE FUNCTION moderation_effect(p_kind text,p_id uuid) RETURNS jsonb LANGUAGE sql VOLATILE AS $$
 SELECT jsonb_build_object('withdrawn',t.withdrawn,'enabled',t.base_enabled AND NOT t.withdrawn AND NOT EXISTS(
     SELECT 1 FROM moderation_holds h WHERE h.target_kind=t.kind AND h.target_id=t.id AND h.active
     AND h.starts_at<=clock_timestamp() AND (h.ends_at IS NULL OR h.ends_at>clock_timestamp()) AND h.effect IN ('hide','remove','restrict')),
   'removed',t.withdrawn OR EXISTS(SELECT 1 FROM moderation_holds h WHERE h.target_kind=t.kind AND h.target_id=t.id AND h.active
     AND h.starts_at<=clock_timestamp() AND (h.ends_at IS NULL OR h.ends_at>clock_timestamp()) AND h.effect='remove'),
   'retired',t.base_retired OR EXISTS(SELECT 1 FROM moderation_holds h WHERE h.target_kind=t.kind AND h.target_id=t.id AND h.active
     AND h.starts_at<=clock_timestamp() AND (h.ends_at IS NULL OR h.ends_at>clock_timestamp()) AND h.effect='retire'),
   'holds',coalesce((SELECT jsonb_agg(jsonb_build_object('case_id',h.case_id,'effect',h.effect,'ends_at',h.ends_at,'authority_order',h.authority_order))
     FROM moderation_holds h WHERE h.target_kind=t.kind AND h.target_id=t.id AND h.active AND h.starts_at<=clock_timestamp()
       AND (h.ends_at IS NULL OR h.ends_at>clock_timestamp())),'[]'::jsonb))
 FROM moderation_targets t WHERE t.kind=p_kind AND t.id=p_id
$$;

CREATE FUNCTION moderation_inbox(p_user uuid) RETURNS jsonb LANGUAGE sql VOLATILE AS $$
 SELECT coalesce(jsonb_agg(jsonb_build_object('id',m.id,'case_id',m.case_id,'decision_id',m.decision_id,'audience',m.audience,
   'statement',m.body,'available_at',m.available_at,'informed_at',m.informed_at,'complaint_until',m.complaint_until,
   'current',m.decision_id IS NOT DISTINCT FROM c.latest_decision,
   'effective',CASE WHEN m.audience='author' THEN moderation_effect(c.target_kind,c.target_id)-'holds' END,
   'content',CASE WHEN m.audience='author' THEN coalesce(d.request->'reviewed_content',c.private_evidence->'target_content') END)
   ORDER BY m.available_at DESC),'[]'::jsonb)
 FROM moderation_messages m JOIN moderation_cases c ON c.id=m.case_id LEFT JOIN moderation_decisions d ON d.id=m.decision_id
 WHERE m.recipient=p_user AND m.available_at<=clock_timestamp()
$$;

CREATE FUNCTION moderation_open(p_id uuid,p_actor uuid,p_kind text,p_target uuid,p_subject uuid,p_source text,p_notifier uuid,p_evidence jsonb)
RETURNS uuid LANGUAGE plpgsql AS $$
DECLARE old moderation_cases;
BEGIN
    IF p_notifier IS NULL AND p_evidence ? 'notifier_contact' THEN p_notifier:=md5('moderation-notifier:'||p_id::text)::uuid; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('moderation:'||p_kind||':'||p_target,0));
    IF EXISTS(SELECT 1 FROM moderation_disposals WHERE case_id=p_id) THEN RAISE EXCEPTION 'Disposed case identity cannot be reused'; END IF;
    SELECT * INTO old FROM moderation_cases WHERE id=p_id;
    IF FOUND THEN
      IF old.target_kind<>p_kind OR old.target_id<>p_target OR old.subject<>p_subject OR old.source<>p_source
        OR old.notifier IS DISTINCT FROM p_notifier OR old.private_evidence<>p_evidence THEN RAISE EXCEPTION 'Conflicting case identity'; END IF;
      RETURN p_id;
    END IF;
    PERFORM moderation_adopt_target(p_kind,p_target,p_subject);
    INSERT INTO moderation_targets(kind,id,subject) VALUES(p_kind,p_target,p_subject) ON CONFLICT DO NOTHING;
    IF NOT EXISTS(SELECT 1 FROM moderation_targets WHERE kind=p_kind AND id=p_target AND subject=p_subject AND NOT withdrawn)
      THEN RAISE EXCEPTION 'Target unavailable or wrong subject'; END IF;
    IF p_source='authority_order' AND (length(coalesce(p_evidence->>'authority',''))<3 OR length(coalesce(p_evidence->>'order_reference',''))<3
      OR length(coalesce(p_evidence->>'notification_instructions',''))<3) THEN RAISE EXCEPTION 'Validated order and instructions required'; END IF;
    INSERT INTO moderation_cases(id,target_kind,target_id,subject,source,created_by,notifier,private_evidence,review_due_at)
      VALUES(p_id,p_kind,p_target,p_subject,p_source,p_actor,p_notifier,p_evidence,clock_timestamp()+interval '7 days');
    IF p_notifier IS NOT NULL OR p_evidence ? 'notifier_contact' THEN
      INSERT INTO moderation_messages(id,case_id,recipient,audience,body) VALUES(gen_random_uuid(),p_id,p_notifier,'notifier',
        jsonb_build_object('status','received','text','Deine Meldung ist eingegangen. Ein Mensch prüft sie.','automation','Der Eingang wurde automatisch gespeichert. Noch keine menschliche Entscheidung.',
        'redress','Nach Information über unsere Entscheidung kannst du mindestens sechs Kalendermonate kostenlos eine menschliche Überprüfung über diesen Vorgang oder hallo@bootstrap.academy verlangen. Auch spätere Beschwerden werden zur menschlichen Prüfung angenommen. Andere Rechtsbehelfe bleiben unberührt.'));
    END IF;
    RETURN p_id;
END $$;

CREATE FUNCTION moderation_decide(p_actor uuid,p_request jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c moderation_cases; prior moderation_decisions; d uuid:=gen_random_uuid(); k uuid:=(p_request->>'request_key')::uuid;
    outcome text:=p_request->>'outcome'; effect text; deadline timestamptz; available timestamptz:=clock_timestamp();
    public jsonb; result jsonb; required text; historical boolean:=false; previous_sanctions integer; ladder_days integer;
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
    IF c.target_kind='subtask' AND (p_request->>'reviewed_content_revision')::bigint IS DISTINCT FROM (SELECT content_revision FROM moderation_targets WHERE kind=c.target_kind AND id=c.target_id) THEN RAISE EXCEPTION 'Content changed or was not reviewed; reload the exact target'; END IF;
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
    IF c.target_kind='account' AND outcome IN ('restrict','uphold') THEN
      FOREACH required IN ARRAY ARRAY['misconduct_facts','proportionality','hearing'] LOOP
       IF length(trim(coalesce(p_request->>required,'')))<3 THEN RAISE EXCEPTION 'Specific account grounds, proportionality and hearing or urgency assessment required'; END IF;
      END LOOP;
    END IF;
    IF c.target_kind IN ('create','report') AND outcome IN ('restrict','uphold') THEN
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
    IF c.target_kind IN ('create','report','account') AND outcome IN ('restrict','uphold') AND p_request->>'article23_applicability'='applies' THEN
      FOREACH required IN ARRAY ARRAY['article23_basis','misconduct_facts','proportionality','prior_warning','absolute_frequency','relative_frequency','seriousness','intent_assessment'] LOOP
       IF length(trim(coalesce(p_request->>required,'')))<1 THEN RAISE EXCEPTION 'Applicable Article 23 suspension requires substantiated applicability, prior warning and individual misuse assessment'; END IF;
      END LOOP;
    END IF;
    IF (p_request->>'scope') IS DISTINCT FROM (CASE c.target_kind WHEN 'subtask' THEN 'Diese Teilaufgabe auf Bootstrap Academy' WHEN 'create' THEN 'Erstellen von Teilaufgaben auf Bootstrap Academy' WHEN 'report' THEN 'Melden von Teilaufgaben auf Bootstrap Academy' ELSE 'Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten' END) THEN RAISE EXCEPTION 'Unsupported scope; do not claim an unimplemented restriction'; END IF;
    deadline:=nullif(p_request->>'ends_at','')::timestamptz;
    IF c.target_kind IN ('create','report') AND outcome IN ('restrict','uphold') THEN
      IF p_request->>'duration_policy'='published_ladder' THEN
        IF outcome='uphold' AND EXISTS(SELECT 1 FROM moderation_holds WHERE case_id=c.id) THEN SELECT ends_at INTO deadline FROM moderation_holds WHERE case_id=c.id;
        ELSE deadline:=clock_timestamp()+make_interval(days=>ladder_days); END IF;
      END IF;
      IF c.target_kind='report' AND deadline IS NULL AND (p_request->>'article23_applicability' IS DISTINCT FROM 'does_not_apply' OR length(trim(coalesce(p_request->>'article23_basis','')))<3) THEN RAISE EXCEPTION 'A finite report restriction is required unless non-application is actually established'; END IF;
    END IF;
    IF c.target_kind IN ('create','report','account') AND outcome IN ('restrict','uphold') AND p_request->>'article23_applicability'='applies' AND deadline IS NULL THEN RAISE EXCEPTION 'Applicable Article 23 suspension requires an assessed finite period'; END IF;
    historical:=outcome='uphold' AND p_request ? 'complaint_id' AND (EXISTS(SELECT 1 FROM moderation_targets WHERE kind=c.target_kind AND id=c.target_id AND withdrawn) OR EXISTS(SELECT 1 FROM moderation_holds WHERE case_id=c.id AND ends_at<=clock_timestamp()));
    IF NOT historical AND deadline IS NOT NULL AND deadline<=clock_timestamp() THEN RAISE EXCEPTION 'Restriction end must be in the future'; END IF;
    effect:=CASE outcome WHEN 'remove' THEN 'remove' WHEN 'retire' THEN 'retire' WHEN 'restrict' THEN 'restrict'
      WHEN 'authority_start' THEN CASE WHEN c.target_kind='subtask' THEN 'hide' ELSE 'restrict' END
      WHEN 'authority_change' THEN CASE WHEN c.target_kind='subtask' THEN 'hide' ELSE 'restrict' END
      WHEN 'provisional' THEN 'hide' WHEN 'uphold' THEN CASE WHEN c.target_kind='subtask' THEN 'hide' ELSE 'restrict' END END;
    IF effect IS NOT NULL AND NOT historical THEN
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
      'article23_applicability',coalesce(p_request->>'article23_applicability','undetermined'),'article23_basis',p_request->>'article23_basis','misconduct_facts',p_request->>'misconduct_facts','proportionality',p_request->>'proportionality','duration_policy',p_request->>'duration_policy','previous_unrescinded_sanctions',previous_sanctions,'hearing',p_request->>'hearing','human_review',p_request->'human_review','review_assessment',p_request->>'review_assessment','historical_only',historical,'effective',moderation_effect(c.target_kind,c.target_id)-'holds');
    INSERT INTO moderation_decisions(id,case_id,actor,request_key,request,outcome,public_statement) VALUES(d,c.id,p_actor,k,p_request,outcome,public);
    UPDATE moderation_cases SET revision=revision+1,latest_decision=d,
      closed_at=NULL,disposition_at=CASE WHEN outcome IN ('restore','authority_end','warn') OR historical THEN clock_timestamp() END,
      review_due_at=CASE WHEN outcome='provisional' THEN clock_timestamp()+interval '7 days' END WHERE id=c.id;
    IF historical THEN UPDATE moderation_holds SET active=false WHERE case_id=c.id; END IF;
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
    IF EXISTS(SELECT 1 FROM moderation_complaints WHERE case_id=c.id AND outcome_decision IS NULL) OR moderation_pending_escalation(c.id) OR moderation_pending_work(c.id) THEN
      UPDATE moderation_cases SET closed_at=NULL,review_due_at=coalesce(review_due_at,clock_timestamp()) WHERE id=c.id;
    END IF;
    RETURN public;
END $$;

CREATE FUNCTION moderation_complain(p_user uuid,p_request jsonb) RETURNS uuid LANGUAGE plpgsql AS $$
DECLARE existing moderation_complaints; m moderation_messages; new_id uuid:=(p_request->>'id')::uuid;
BEGIN
    IF new_id IS NULL OR p_user IS NULL OR (p_request->>'decision_id') IS NULL OR length(trim(coalesce(p_request->>'text','')))<1 THEN RAISE EXCEPTION 'Complete complaint identity and text required'; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('complaint:'||new_id,0));
    SELECT * INTO existing FROM moderation_complaints WHERE moderation_complaints.id=new_id;
    IF FOUND THEN
      IF existing.complainant IS DISTINCT FROM p_user OR existing.decision_id IS DISTINCT FROM (p_request->>'decision_id')::uuid OR existing.text IS DISTINCT FROM p_request->>'text'
        THEN RAISE EXCEPTION 'Conflicting complaint replay'; END IF;
      RETURN new_id;
    END IF;
    SELECT * INTO m FROM moderation_messages WHERE decision_id=(p_request->>'decision_id')::uuid AND recipient=p_user AND available_at<=clock_timestamp() LIMIT 1;
    IF NOT FOUND THEN RAISE EXCEPTION 'Decision unavailable for this recipient'; END IF;
    -- Late complaints are retained for human assessment; this never limits other remedies.
    INSERT INTO moderation_complaints(id,case_id,decision_id,complainant,text) VALUES(new_id,m.case_id,m.decision_id,p_user,p_request->>'text');
    UPDATE moderation_cases SET closed_at=NULL,review_due_at=clock_timestamp()+interval '14 days' WHERE moderation_cases.id=m.case_id;
    INSERT INTO moderation_messages(id,case_id,recipient,audience,body) VALUES(gen_random_uuid(),m.case_id,p_user,m.audience,jsonb_build_object('status','complaint_received','complaint_id',new_id,'decision_id',m.decision_id,'text','Deine Beschwerde ist eingegangen und wartet auf menschliche Überprüfung.','automation','Nur Eingangsbestätigung; keine automatische Beschwerdeentscheidung.'));
    RETURN new_id;
END $$;

CREATE FUNCTION moderation_withdraw_target(p_kind text,p_target uuid) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 PERFORM pg_advisory_xact_lock(hashtextextended('moderation:'||p_kind||':'||p_target,0));
 UPDATE moderation_targets SET withdrawn=true WHERE kind=p_kind AND id=p_target;
 -- Withdrawal ends ordinary application, without rescinding the historical
 -- merits or pretending that an independent binding order was revoked.
 UPDATE moderation_holds SET active=false WHERE target_kind=p_kind AND target_id=p_target AND NOT authority_order;
 UPDATE moderation_cases SET disposition_at=clock_timestamp(),closed_at=NULL,review_due_at=clock_timestamp() WHERE target_kind=p_kind AND target_id=p_target;
END $$;

CREATE FUNCTION moderation_erase(p_user uuid) RETURNS void LANGUAGE plpgsql AS $$
DECLARE target moderation_targets;
BEGIN
    FOR target IN SELECT * FROM moderation_targets WHERE subject=p_user ORDER BY kind,id LOOP
      PERFORM moderation_withdraw_target(target.kind,target.id);
    END LOOP;
    INSERT INTO moderation_erasure_events(subject) VALUES(p_user) ON CONFLICT DO NOTHING;
    UPDATE moderation_targets SET withdrawn=true WHERE subject=p_user;
    -- Statements/necessary dispute evidence have their own review/retention lifecycle.
    UPDATE moderation_cases SET private_evidence=private_evidence-'notifier_contact' WHERE notifier=p_user;
END $$;

-- Private evidence is available only to the authenticated human case workflow.
CREATE FUNCTION moderation_queue(p_limit integer,p_offset integer) RETURNS jsonb LANGUAGE sql VOLATILE AS $$
 SELECT coalesce(jsonb_agg(to_jsonb(c)||jsonb_build_object(
 'decisions',coalesce((SELECT jsonb_agg(to_jsonb(d) ORDER BY d.created_at) FROM moderation_decisions d WHERE d.case_id=c.id),'[]'),
 'complaints',coalesce((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.received_at) FROM moderation_complaints a WHERE a.case_id=c.id),'[]'),
 'escalations',coalesce((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.recorded_at) FROM moderation_escalations e WHERE e.case_id=c.id),'[]'),
 'effective',moderation_effect(c.target_kind,c.target_id))), '[]')
 FROM (SELECT * FROM moderation_cases ORDER BY closed_at NULLS FIRST,review_due_at NULLS LAST,received_at LIMIT least(greatest(p_limit,1),100) OFFSET greatest(p_offset,0)) c
$$;

CREATE FUNCTION moderation_escalate(p_actor uuid,p_request jsonb) RETURNS uuid LANGUAGE plpgsql AS $$
DECLARE old moderation_escalations; new_id uuid:=(p_request->>'id')::uuid; field text;
BEGIN
 IF p_request->>'kind' IS NULL OR p_request->>'kind' NOT IN ('article18_assessment','article18_transmission','authority_validation','authority_notification','notice_instruction','work_resolution','platform_obligations_review') THEN RAISE EXCEPTION 'Explicit escalation record required'; END IF;
 FOREACH field IN ARRAY ARRAY['facts','assessment','human_responsibility'] LOOP
  IF length(trim(coalesce(p_request->>field,'')))<3 THEN RAISE EXCEPTION 'Escalation facts and human assessment required'; END IF;
 END LOOP;
 IF p_request->>'kind'='article18_transmission' AND (length(coalesce(p_request->>'authority',''))<3 OR length(coalesce(p_request->>'transmission_evidence',''))<3 OR p_request->>'transmitted_at' IS NULL OR (p_request->>'transmitted_at')::timestamptz>clock_timestamp()) THEN RAISE EXCEPTION 'Actual transmission evidence required'; END IF;
 IF p_request->>'kind'='authority_notification' AND (p_request->>'notified_at' IS NULL OR (p_request->>'notified_at')::timestamptz>clock_timestamp() OR length(coalesce(p_request->>'notification_evidence',''))<3) THEN RAISE EXCEPTION 'Actual authority-notification evidence required'; END IF;
 IF p_request->>'kind'='work_resolution' AND NOT EXISTS(SELECT 1 FROM moderation_escalations WHERE id=(p_request->>'resolves')::uuid AND case_id=(p_request->>'case_id')::uuid AND record->>'kind'<>'work_resolution') THEN RAISE EXCEPTION 'Explicit existing work reference required'; END IF;
 IF p_request->>'kind'='notice_instruction' AND ((p_request->>'notice_after')::timestamptz IS NULL OR length(trim(coalesce(p_request->>'instruction_evidence','')))<3 OR NOT EXISTS(SELECT 1 FROM moderation_cases WHERE id=(p_request->>'case_id')::uuid AND source='authority_order')) THEN RAISE EXCEPTION 'Supported independent authority notice instruction required'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('escalation:'||new_id,0));
 SELECT * INTO old FROM moderation_escalations WHERE moderation_escalations.id=new_id;
 IF FOUND THEN
  IF old.actor<>p_actor OR old.record<>p_request THEN RAISE EXCEPTION 'Conflicting escalation replay'; END IF;
  RETURN new_id;
 END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('moderation:'||target_kind||':'||target_id,0)) FROM moderation_cases WHERE id=(p_request->>'case_id')::uuid;
 INSERT INTO moderation_escalations(id,case_id,actor,record) VALUES(new_id,(p_request->>'case_id')::uuid,p_actor,p_request);
 IF p_request->>'kind'='notice_instruction' THEN
  UPDATE moderation_cases SET notice_after=(p_request->>'notice_after')::timestamptz WHERE id=(p_request->>'case_id')::uuid;
  UPDATE moderation_messages SET available_at=greatest(clock_timestamp(),(p_request->>'notice_after')::timestamptz) WHERE case_id=(p_request->>'case_id')::uuid AND available_at>clock_timestamp() AND relayed_at IS NULL;
 END IF;
 UPDATE moderation_cases SET closed_at=NULL,review_due_at=clock_timestamp() WHERE id=(p_request->>'case_id')::uuid;
 RETURN new_id;
END $$;

-- A bounded worker claims records durably; generation prevents stale acknowledgements.
CREATE FUNCTION moderation_claim(p_limit integer) RETURNS jsonb LANGUAGE sql AS $$
 WITH due AS (SELECT id FROM moderation_messages WHERE relayed_at IS NULL AND available_at<=clock_timestamp()
 AND next_attempt_at<=clock_timestamp() AND (lease_until IS NULL OR lease_until<=clock_timestamp())
 ORDER BY available_at FOR UPDATE SKIP LOCKED LIMIT least(greatest(p_limit,1),50)), claimed AS (
 UPDATE moderation_messages m SET generation=generation+1,attempts=attempts+1,lease_until=clock_timestamp()+interval '2 minutes'
 FROM due WHERE m.id=due.id RETURNING m.*)
 SELECT coalesce(jsonb_agg(jsonb_build_object('id',m.id,'case_id',m.case_id,'recipient',m.recipient,'audience',m.audience,'body',m.body,
 'available_at',m.available_at,'complaint_until',m.complaint_until,'generation',m.generation,
 'contact',CASE WHEN m.audience='notifier' THEN c.private_evidence->>'notifier_contact' ELSE c.private_evidence->>'author_contact' END)),'[]')
 FROM claimed m JOIN moderation_cases c ON c.id=m.case_id
$$;
CREATE FUNCTION moderation_ack(p_id uuid,p_generation bigint,p_ok boolean) RETURNS boolean LANGUAGE plpgsql AS $$
BEGIN
 UPDATE moderation_messages SET relayed_at=CASE WHEN p_ok THEN clock_timestamp() END,lease_until=NULL,
  next_attempt_at=clock_timestamp()+make_interval(secs=>least(86400,30*(2^least(attempts,11))::integer)),
  delivery_status=CASE WHEN p_ok THEN 'relay_accepted' ELSE 'retry' END,
  delivery_error=CASE WHEN p_ok THEN NULL ELSE 'Backend relay unavailable; statement remains available in the owning service' END
 WHERE id=p_id AND generation=p_generation AND relayed_at IS NULL AND lease_until>clock_timestamp();
 RETURN FOUND;
END $$;

-- The displayed date is a minimum, never an automatic time bar. Six calendar
-- months begin with evidenced information, not SMTP acceptance or queue creation.
-- Later complaints (including applicable holiday extensions) always reach humans.
CREATE FUNCTION moderation_review_minimum(p_at timestamptz) RETURNS timestamptz LANGUAGE sql IMMUTABLE AS $$
 SELECT (date_trunc('day',p_at AT TIME ZONE 'Europe/Berlin')+interval '6 months 1 day') AT TIME ZONE 'Europe/Berlin'
$$;
CREATE FUNCTION moderation_opened(p_user uuid,p_id uuid) RETURNS boolean LANGUAGE plpgsql AS $$
BEGIN
 UPDATE moderation_messages SET first_opened_at=coalesce(first_opened_at,clock_timestamp()),
  informed_at=coalesce(informed_at,clock_timestamp()),notification_evidence=coalesce(notification_evidence,jsonb_build_object('basis','recipient_opened','recorded_at',clock_timestamp())),
  complaint_until=coalesce(complaint_until,moderation_review_minimum(clock_timestamp()))
 WHERE id=p_id AND recipient=p_user AND available_at<=clock_timestamp();
 RETURN FOUND;
END $$;

CREATE FUNCTION moderation_pending_escalation(p_case uuid) RETURNS boolean LANGUAGE sql VOLATILE AS $$
 SELECT EXISTS(SELECT 1 FROM moderation_escalations e WHERE e.case_id=p_case AND e.record->>'kind'<>'work_resolution'
  AND NOT EXISTS(SELECT 1 FROM moderation_escalations resolved WHERE resolved.case_id=p_case AND resolved.record->>'kind'='work_resolution' AND resolved.record->>'resolves'=e.id::text))
$$;
CREATE FUNCTION moderation_legacy_statements() RETURNS void LANGUAGE plpgsql AS $$
DECLARE c moderation_cases; d uuid; statement jsonb;
BEGIN
 FOR c IN SELECT * FROM moderation_cases WHERE source='legacy_import' AND latest_decision IS NULL LOOP
  d:=gen_random_uuid();
  statement:=jsonb_build_object('decision_id',d,'case_id',c.id,'target_kind',c.target_kind,'target_id',c.target_id,'outcome','legacy_observed','decided_at',clock_timestamp(),
   'rationale','Dieser Vorgang wurde aus einem früheren System übernommen. Der gespeicherte Stand wird zur menschlichen Prüfung bereitgestellt. Eine frühere konkrete Begründung oder Benachrichtigung ist hier nicht nachgewiesen; damit wird kein neuer Regelverstoß festgestellt.',
   'ground','Historische Grundlage nicht festgestellt; menschliche Prüfung erforderlich','rule_version','Historische Fassung und Anwendbarkeit nicht festgestellt',
   'automation','Automatische Übernahme des vorhandenen Zustands; keine neue menschliche Sachentscheidung',
   'scope',CASE c.target_kind WHEN 'subtask' THEN 'Diese Teilaufgabe auf Bootstrap Academy' WHEN 'create' THEN 'Erstellen von Teilaufgaben auf Bootstrap Academy' WHEN 'report' THEN 'Melden von Teilaufgaben auf Bootstrap Academy' ELSE 'Allgemeiner Kontozugang auf Bootstrap Academy; Rechtezugang bleibt erhalten' END,
   'redress','Kostenlose menschliche Überprüfung unter /moderation oder hallo@bootstrap.academy für mindestens sechs Monate ab Information. Spätere Beschwerden werden ebenfalls zur menschlichen Prüfung angenommen. Gesetzliche Rechtsbehelfe bleiben unberührt.',
   'effective',moderation_effect(c.target_kind,c.target_id)-'holds');
  INSERT INTO moderation_decisions(id,case_id,actor,request_key,request,outcome,public_statement) VALUES(d,c.id,'00000000-0000-0000-0000-000000000000',d,jsonb_build_object('basis','current_legacy_observation'),'legacy_observed',statement);
  UPDATE moderation_cases SET latest_decision=d,revision=1,review_due_at=clock_timestamp() WHERE id=c.id;
  INSERT INTO moderation_messages(id,case_id,decision_id,recipient,audience,body) VALUES(gen_random_uuid(),c.id,d,c.subject,'author',statement);
  IF c.notifier IS NOT NULL THEN INSERT INTO moderation_messages(id,case_id,decision_id,recipient,audience,body) VALUES(gen_random_uuid(),c.id,d,c.notifier,'notifier',jsonb_build_object('decision_id',d,'case_id',c.id,'outcome','legacy_observed','text','Deine frühere Meldung wurde übernommen. Ein früheres Ergebnis und eine frühere Benachrichtigung sind hier nicht nachgewiesen. Menschliche Prüfung steht aus.','automation',statement->'automation','redress',statement->'redress')); END IF;
 END LOOP;
END $$;

-- Bounded recovery of elapsed measures. Original end and decision stay intact.
CREATE FUNCTION moderation_maintenance() RETURNS integer LANGUAGE plpgsql AS $$
DECLARE c moderation_cases; h moderation_holds; previous moderation_decisions; cmd jsonb; n integer:=0;
BEGIN
 FOR h IN SELECT * FROM moderation_holds WHERE active AND ends_at<=clock_timestamp() ORDER BY target_kind,target_id,case_id LIMIT 25 LOOP
  PERFORM moderation_lock_target(h.target_kind,h.target_id);
  PERFORM pg_advisory_xact_lock(hashtextextended('moderation:'||h.target_kind||':'||h.target_id,0));
  SELECT * INTO h FROM moderation_holds WHERE case_id=h.case_id AND active AND ends_at<=clock_timestamp() FOR UPDATE;
  IF NOT FOUND THEN CONTINUE; END IF;
  SELECT * INTO c FROM moderation_cases WHERE id=h.case_id FOR UPDATE;
  SELECT * INTO previous FROM moderation_decisions WHERE id=c.latest_decision;
  cmd:=jsonb_build_object('request_key',gen_random_uuid(),'case_id',c.id,'expected_revision',c.revision,'reviewed_content_revision',(SELECT content_revision FROM moderation_targets WHERE kind=c.target_kind AND id=c.target_id),
   'expired_measure',true,'outcome',CASE WHEN h.authority_order THEN 'authority_end' ELSE 'restore' END,
   'rationale','Das gespeicherte Ende dieser Einschränkung ist erreicht. Diese Einschränkung wird beendet. Andere Einschränkungen und ein Rückzug durch den Autor bleiben maßgeblich.',
   'ground','Ablauf der zuvor ausdrücklich festgelegten Dauer; keine neue Prüfung des ursprünglichen Vorwurfs','rule_version',coalesce(previous.public_statement->>'rule_version','Historische Grundlage nicht festgestellt'),
   'automation','Automatischer Vollzug des gespeicherten Endes; keine automatisierte Beschwerdeentscheidung',
   'scope',previous.public_statement->>'scope','redress',previous.public_statement->>'redress','order_event_evidence','Gespeichertes Ende der Anordnung: '||h.ends_at::text);
  PERFORM moderation_decide('00000000-0000-0000-0000-000000000000',cmd);n:=n+1;
 END LOOP;
 UPDATE moderation_cases closing_case SET closed_at=clock_timestamp(),review_due_at=NULL
 WHERE closed_at IS NULL AND disposition_at IS NOT NULL AND NOT EXISTS(SELECT 1 FROM moderation_holds WHERE case_id=closing_case.id AND active)
 AND NOT EXISTS(SELECT 1 FROM moderation_complaints WHERE case_id=closing_case.id AND outcome_decision IS NULL) AND NOT moderation_pending_escalation(closing_case.id) AND NOT moderation_pending_work(closing_case.id)
 AND NOT EXISTS(SELECT 1 FROM moderation_messages WHERE case_id=closing_case.id AND decision_id IS NOT NULL AND (informed_at IS NULL OR complaint_until>clock_timestamp()));
 RETURN n;
END $$;

CREATE TABLE moderation_retention_reviews (
 id uuid PRIMARY KEY,case_id uuid NOT NULL,actor uuid NOT NULL,recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),record jsonb NOT NULL
);
CREATE TRIGGER moderation_retention_reviews_immutable BEFORE UPDATE OR DELETE ON moderation_retention_reviews FOR EACH ROW EXECUTE FUNCTION moderation_immutable();
CREATE TABLE moderation_disposals (
 case_id uuid PRIMARY KEY,request_id uuid NOT NULL UNIQUE,command_sha256 text NOT NULL,disposed_at timestamptz NOT NULL DEFAULT clock_timestamp(),actor uuid NOT NULL,reason text NOT NULL,
 relayed_at timestamptz,review_due_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '12 months'
);
CREATE FUNCTION moderation_retention(p_actor uuid,p_body jsonb) RETURNS boolean LANGUAGE plpgsql AS $$
DECLARE c moderation_cases; field text; review_id uuid:=(p_body->>'id')::uuid; prior moderation_retention_reviews; disposed moderation_disposals;
BEGIN
 IF review_id IS NULL OR p_actor IS NULL OR length(trim(coalesce(p_body->>'reason','')))<3 OR p_body->>'action' IS NULL OR p_body->>'action' NOT IN ('retain','release_retention','minimize','dispose') THEN RAISE EXCEPTION 'Actual retention assessment required'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('retention:'||review_id,0));
 SELECT * INTO disposed FROM moderation_disposals WHERE request_id=review_id;
 IF FOUND THEN IF disposed.actor IS DISTINCT FROM p_actor OR disposed.command_sha256 IS DISTINCT FROM encode(sha256(convert_to(p_body::text,'UTF8')),'hex') THEN RAISE EXCEPTION 'Conflicting disposal replay'; END IF; RETURN true; END IF;
 SELECT * INTO prior FROM moderation_retention_reviews WHERE id=review_id;
 IF FOUND THEN IF prior.actor<>p_actor OR prior.record<>p_body THEN RAISE EXCEPTION 'Conflicting retention replay'; END IF; RETURN true; END IF;
 SELECT * INTO c FROM moderation_cases WHERE id=(p_body->>'case_id')::uuid;
 IF NOT FOUND THEN RAISE EXCEPTION 'Case unavailable'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('moderation:'||c.target_kind||':'||c.target_id,0));
 SELECT * INTO c FROM moderation_cases WHERE id=c.id FOR UPDATE;
 IF p_body->>'action'='retain' AND ((p_body->>'review_at')::timestamptz IS NULL OR (p_body->>'review_at')::timestamptz<=clock_timestamp() OR length(trim(coalesce(p_body->>'legal_or_claim_basis','')))<3 OR jsonb_typeof(p_body->'necessary_fields') IS DISTINCT FROM 'array') THEN RAISE EXCEPTION 'Document a concrete basis, necessary scope and future retention review'; END IF;
 IF p_body->>'action'='release_retention' AND NOT EXISTS(SELECT 1 FROM moderation_retention_reviews WHERE id=(p_body->>'retention_id')::uuid AND case_id=c.id AND record->>'action'='retain') THEN RAISE EXCEPTION 'Existing retention exception required'; END IF;
 IF p_body->>'action'='dispose' THEN
  IF EXISTS(SELECT 1 FROM moderation_retention_reviews r WHERE r.case_id=c.id AND r.record->>'action'='retain' AND NOT EXISTS(SELECT 1 FROM moderation_retention_reviews release WHERE release.case_id=c.id AND release.record->>'action'='release_retention' AND release.record->>'retention_id'=r.id::text)) THEN RAISE EXCEPTION 'Review and explicitly release the documented retention exception'; END IF;
  IF c.closed_at IS NULL OR c.closed_at+interval '12 months'>clock_timestamp() OR EXISTS(SELECT 1 FROM moderation_holds WHERE case_id=c.id AND active)
   OR EXISTS(SELECT 1 FROM moderation_complaints WHERE case_id=c.id AND outcome_decision IS NULL) OR moderation_pending_escalation(c.id)
   OR p_body->'claims_and_retention_checked' IS DISTINCT FROM 'true'::jsonb THEN RAISE EXCEPTION 'Default retention or unresolved rights/work prevents disposal'; END IF;
 END IF;
 INSERT INTO moderation_retention_reviews(id,case_id,actor,record) VALUES(review_id,c.id,p_actor,p_body);
 IF p_body->>'action'='minimize' THEN
  IF jsonb_typeof(p_body->'unnecessary_fields') IS DISTINCT FROM 'array' THEN RAISE EXCEPTION 'Explicit unnecessary private fields required'; END IF;
  FOR field IN SELECT jsonb_array_elements_text(p_body->'unnecessary_fields') LOOP
   IF field NOT IN ('comment','attachments','reason','notifier_contact','author_contact') THEN RAISE EXCEPTION 'Unsupported evidence minimization'; END IF;
   IF field IN ('notifier_contact','author_contact') AND c.closed_at IS NULL THEN RAISE EXCEPTION 'Current remedy contact remains necessary; correct it through verified contact handling'; END IF;
   IF EXISTS(SELECT 1 FROM moderation_retention_reviews r WHERE r.case_id=c.id AND r.record->>'action'='retain' AND r.record->'necessary_fields' ? field AND NOT EXISTS(SELECT 1 FROM moderation_retention_reviews released WHERE released.case_id=c.id AND released.record->>'action'='release_retention' AND released.record->>'retention_id'=r.id::text)) THEN RAISE EXCEPTION 'Release or amend the applicable field retention before minimization'; END IF;
   IF EXISTS(SELECT 1 FROM moderation_complaints WHERE case_id=c.id AND outcome_decision IS NULL) AND p_body->'open_complaints_considered' IS DISTINCT FROM 'true'::jsonb THEN RAISE EXCEPTION 'Assess the specific open complaints before evidence minimization'; END IF;
   UPDATE moderation_cases SET private_evidence=CASE WHEN field='author_contact' THEN (private_evidence-field)#-'{rule_evidence,contact}' ELSE private_evidence-field END WHERE id=c.id;
   PERFORM moderation_minimize_adapter(c.id,field);

  END LOOP;
 ELSIF p_body->>'action'='dispose' THEN
  INSERT INTO moderation_disposals(case_id,request_id,command_sha256,actor,reason) VALUES(c.id,review_id,encode(sha256(convert_to(p_body::text,'UTF8')),'hex'),p_actor,'Reviewed disposal completed; canonical request digest retained for exact replay and protection against stale reimport');
  PERFORM moderation_disposal_adapter(c.id);
  PERFORM set_config('academy.moderation_disposal','authorized',true);
  DELETE FROM moderation_complaints WHERE case_id=c.id;
  DELETE FROM moderation_messages WHERE case_id=c.id;
  DELETE FROM moderation_holds WHERE case_id=c.id;
  DELETE FROM moderation_escalations WHERE case_id=c.id;
  DELETE FROM moderation_decisions WHERE case_id=c.id;
  DELETE FROM moderation_cases WHERE id=c.id;
  DELETE FROM moderation_retention_reviews WHERE case_id=c.id;
  DELETE FROM moderation_targets WHERE kind=c.target_kind AND id=c.target_id AND NOT EXISTS(SELECT 1 FROM moderation_cases WHERE target_kind=c.target_kind AND target_id=c.target_id);
  PERFORM set_config('academy.moderation_disposal','',true);
 END IF;
 RETURN true;
END $$;

-- Backend adapter. Account restrictions retain commercial and rights records.
CREATE TABLE moderation_capabilities (
 hash text PRIMARY KEY CHECK(length(hash)=64),subject uuid NOT NULL,case_id uuid,
 scope text NOT NULL CHECK(scope IN ('rights','case')),source text CHECK(source IN ('backend','challenges')),expires_at timestamptz NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), revoked_at timestamptz
);
CREATE TABLE moderation_recovery_attempts (
 ip_hash text NOT NULL,requested_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE moderation_delivery (
 source text NOT NULL CHECK(source IN ('backend','challenges')),id uuid NOT NULL,case_id uuid NOT NULL,
 recipient uuid, audience text NOT NULL, body jsonb NOT NULL,contact text,owner_contact text,owner_contact_digest text,
 owner_available_at timestamptz NOT NULL,accepted_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 delivered_at timestamptz,opened_at timestamptz,status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','in_flight','retry','uncertain','no_contact','expired','transport_accepted')),
 attempts integer NOT NULL DEFAULT 0,generation bigint NOT NULL DEFAULT 0,next_attempt_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 lease_until timestamptz,PRIMARY KEY(source,id)
);
CREATE TABLE moderation_external_disposals (source text NOT NULL,case_id uuid NOT NULL,received_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(source,case_id));
CREATE TABLE moderation_recipient_contacts (
 source text NOT NULL,case_id uuid NOT NULL,recipient uuid NOT NULL,audience text NOT NULL,
 contact text,updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),basis text NOT NULL,
 PRIMARY KEY(source,case_id,recipient,audience)
);
CREATE TABLE moderation_delivery_events (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, source text NOT NULL,message_id uuid NOT NULL, generation bigint NOT NULL,
 event text NOT NULL,recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(), evidence jsonb NOT NULL DEFAULT '{}'
);
CREATE TRIGGER moderation_delivery_events_immutable BEFORE UPDATE OR DELETE ON moderation_delivery_events FOR EACH ROW EXECUTE FUNCTION moderation_immutable();
CREATE TABLE moderation_handling (
 case_id uuid NOT NULL REFERENCES moderation_cases(id),kind text NOT NULL,
 status text NOT NULL DEFAULT 'pending_human_handling',record jsonb NOT NULL DEFAULT '{}',
 PRIMARY KEY(case_id,kind)
);
CREATE TABLE moderation_handling_events (
 id uuid PRIMARY KEY,case_id uuid NOT NULL,actor uuid NOT NULL,record jsonb NOT NULL,recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER moderation_handling_events_immutable BEFORE UPDATE OR DELETE ON moderation_handling_events FOR EACH ROW EXECUTE FUNCTION moderation_immutable();
CREATE FUNCTION moderation_lock_target(p_kind text,p_id uuid) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF p_kind<>'account' THEN RAISE EXCEPTION 'Unsupported backend target'; END IF;
 PERFORM 1 FROM users WHERE id=p_id FOR UPDATE;
END $$;
CREATE FUNCTION moderation_project(p_kind text,p_id uuid) RETURNS void LANGUAGE plpgsql AS $$
DECLARE state jsonb; old_guard text:=current_setting('academy.moderation_write',true);
BEGIN
 IF p_kind<>'account' THEN RAISE EXCEPTION 'Unsupported backend target'; END IF;
 PERFORM 1 FROM users WHERE id=p_id FOR UPDATE;
 state:=moderation_effect(p_kind,p_id);
 PERFORM set_config('academy.moderation_write','authorized',true);
 UPDATE users SET enabled=(state->>'enabled')::boolean WHERE id=p_id;
 IF NOT (state->>'enabled')::boolean THEN
  DELETE FROM sessions WHERE user_id=p_id;
  INSERT INTO moderation_handling(case_id,kind)
   SELECT id,'commercial_rights_and_contract_review' FROM moderation_cases WHERE target_kind=p_kind AND target_id=p_id
   ON CONFLICT DO NOTHING;
 END IF;
 PERFORM set_config('academy.moderation_write',coalesce(old_guard,''),true);
END $$;
-- Existing disabled accounts remain disabled, with explicitly unknown historic notice facts.
INSERT INTO moderation_targets(kind,id,subject) SELECT 'account',id,id FROM users WHERE NOT enabled;
INSERT INTO moderation_cases(id,target_kind,target_id,subject,source,private_evidence,review_due_at)
 SELECT gen_random_uuid(),'account',id,id,'legacy_import',jsonb_build_object('observed_enabled',enabled,'author_contact',email,'historical_ground','unknown','historical_notification','unknown'),clock_timestamp()
 FROM users WHERE NOT enabled;
INSERT INTO moderation_holds(case_id,target_kind,target_id,effect,starts_at)
 SELECT id,'account',target_id,'restrict',clock_timestamp() FROM moderation_cases;
DELETE FROM sessions WHERE user_id IN (SELECT id FROM users WHERE NOT enabled);
SELECT moderation_legacy_statements();

-- Account reads use current owning holds, so a finite measure ends even when
-- background disposition/notification work is delayed. Preserve the stored
-- flag for accounts without a managed target (including unknown legacy state).
DO $$ DECLARE columns text; BEGIN
 SELECT string_agg(CASE WHEN attname='enabled' THEN
   'coalesce((moderation_effect(''account'',stored.id)->>''enabled'')::boolean,stored.enabled) AS enabled'
   ELSE format('stored.%I',attname) END,',' ORDER BY attnum) INTO columns
 FROM pg_attribute WHERE attrelid='users'::regclass AND attnum>0 AND NOT attisdropped;
 -- Keep id/name/email as ordinary indexed columns; do not deserialize every
 -- account through a JSON record to answer one authentication lookup.
 EXECUTE 'CREATE OR REPLACE VIEW user_composites AS SELECT * FROM (SELECT '||columns||' FROM users stored) u
  INNER JOIN user_profiles p ON u.id=p.user_id INNER JOIN user_details d USING(user_id)
  INNER JOIN user_invoice_info i USING(user_id)';
END $$;

-- Adopting an unmanaged current restriction must not itself restore access.
-- Record the observed state separately; no historical finding or notice is inferred.
CREATE FUNCTION moderation_adopt_target(p_kind text,p_id uuid,p_subject uuid) RETURNS void LANGUAGE plpgsql AS $$
DECLARE observed users; legacy uuid:=gen_random_uuid();
BEGIN
 IF EXISTS(SELECT 1 FROM moderation_targets WHERE kind=p_kind AND id=p_id) THEN RETURN; END IF;
 IF p_kind<>'account' THEN RAISE EXCEPTION 'Unsupported backend target'; END IF;
 SELECT * INTO observed FROM users WHERE id=p_id FOR UPDATE;
 IF NOT FOUND OR observed.id<>p_subject THEN RAISE EXCEPTION 'Target unavailable'; END IF;
 INSERT INTO moderation_targets(kind,id,subject) VALUES(p_kind,p_id,p_subject);
 IF NOT observed.enabled THEN
  INSERT INTO moderation_cases(id,target_kind,target_id,subject,source,private_evidence,review_due_at)
   VALUES(legacy,p_kind,p_id,p_subject,'legacy_import',jsonb_build_object('observed_enabled',false,'observed_at',clock_timestamp(),'author_contact',observed.email,'historical_ground','unknown','historical_notification','unknown'),clock_timestamp());
  INSERT INTO moderation_holds(case_id,target_kind,target_id,effect,starts_at) VALUES(legacy,p_kind,p_id,'restrict',clock_timestamp());
  PERFORM moderation_legacy_statements();
 END IF;
END $$;

CREATE FUNCTION moderation_user_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE contact_case record;
BEGIN
 IF TG_OP='DELETE' THEN
  IF current_setting('academy.moderation_erasure_subject',true) IS DISTINCT FROM OLD.id::text THEN RAISE EXCEPTION 'Account removal requires authenticated self-erasure'; END IF;
  PERFORM moderation_erase(OLD.id);
  RETURN OLD;
 END IF;
 IF (NEW.email,NEW.email_verified) IS DISTINCT FROM (OLD.email,OLD.email_verified) THEN
  FOR contact_case IN SELECT DISTINCT source,case_id FROM moderation_recipient_contacts WHERE recipient=OLD.id AND audience='author' ORDER BY source,case_id LOOP
   PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:'||contact_case.source||':'||contact_case.case_id,0));
  END LOOP;
  UPDATE moderation_recipient_contacts SET contact=CASE WHEN NEW.email_verified THEN NEW.email END,updated_at=clock_timestamp(),basis='current_account_contact_verification' WHERE recipient=OLD.id AND audience='author';
  UPDATE moderation_capabilities SET revoked_at=clock_timestamp() WHERE subject=OLD.id AND scope='case' AND revoked_at IS NULL;
 END IF;
 IF NEW.enabled IS DISTINCT FROM OLD.enabled AND current_setting('academy.moderation_write',true) IS DISTINCT FROM 'authorized' THEN
  RAISE EXCEPTION 'Use a complete moderation decision to restrict or restore an account';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER moderation_user_guard BEFORE UPDATE OR DELETE ON users FOR EACH ROW EXECUTE FUNCTION moderation_user_guard();
-- Creation/refresh cannot race a committed restriction. This locks only the
-- account row, not commercial balances, and never changes existence semantics.
CREATE FUNCTION moderation_session_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE permitted boolean;
BEGIN
 PERFORM 1 FROM users WHERE id=NEW.user_id FOR UPDATE;
 SELECT enabled INTO permitted FROM user_composites WHERE id=NEW.user_id;
 IF permitted IS DISTINCT FROM true THEN RAISE EXCEPTION 'Ordinary session unavailable for restricted account'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER moderation_session_guard BEFORE INSERT OR UPDATE ON sessions FOR EACH ROW EXECUTE FUNCTION moderation_session_guard();

CREATE FUNCTION backend_moderation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE target_subject uuid; source_name text; contact_minimized boolean; old moderation_delivery; cap moderation_capabilities; claimed_record moderation_delivery; claimed_messages jsonb:='[]';
BEGIN
 CASE p_operation
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
  RETURN to_jsonb(moderation_open((p_body->>'id')::uuid,p_actor,'account',target_subject,target_subject,p_body->>'source',(p_body->>'notifier')::uuid,coalesce(p_body->'private_evidence','{}')||jsonb_build_object('author_contact',(SELECT email FROM users WHERE id=target_subject),'recorded_terms_version',(SELECT terms_version FROM users WHERE id=target_subject),'recorded_acceptance_at',(SELECT terms_accepted_at FROM users WHERE id=target_subject),'contract_validity','not_assessed')));
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
  SELECT m.* INTO old FROM moderation_recipient_contacts c JOIN moderation_delivery m ON (m.source,m.case_id,m.recipient,m.audience)=(c.source,c.case_id,c.recipient,c.audience) WHERE c.source=p_body->>'source' AND c.case_id=(p_body->>'case_id')::uuid AND lower(trim(c.contact))=lower(trim(p_body->>'contact')) AND c.audience IN ('author','notifier') ORDER BY m.accepted_at DESC LIMIT 1;
  IF NOT FOUND THEN RETURN 'true'; END IF;
  old.contact:=(SELECT contact FROM moderation_recipient_contacts WHERE (source,case_id,recipient,audience)=(old.source,old.case_id,old.recipient,old.audience));
  IF EXISTS(SELECT 1 FROM moderation_capabilities WHERE subject=old.recipient AND case_id=old.case_id AND source=old.source AND revoked_at IS NULL AND expires_at>clock_timestamp() AND created_at>clock_timestamp()-interval '5 minutes') THEN RETURN 'true'; END IF;
  INSERT INTO moderation_capabilities(hash,subject,case_id,scope,source,expires_at) VALUES(p_body->>'hash',old.recipient,old.case_id,'case',old.source,clock_timestamp()+interval '30 minutes');
  INSERT INTO moderation_delivery(source,id,case_id,recipient,audience,body,contact,owner_available_at)
   VALUES('backend',gen_random_uuid(),old.case_id,old.recipient,'recovery',jsonb_build_object('recovery_link',p_body->>'link','text','Ein Zugang zu diesem Vorgang wurde angefordert. Der Link gilt 30 Minuten und berechtigt nur zum Lesen dieses Vorgangs und zu einer Beschwerde. Falls du ihn nicht angefordert hast, musst du nichts tun.','expires_at',clock_timestamp()+interval '30 minutes','case_source',old.source),old.contact,clock_timestamp());
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
  UPDATE moderation_recipient_contacts SET contact=p_body->>'contact',updated_at=clock_timestamp(),basis='verified_case_contact_correction' WHERE (source,case_id,recipient,audience)=(old.source,old.case_id,old.recipient,old.audience);
  UPDATE moderation_capabilities SET revoked_at=clock_timestamp() WHERE subject=old.recipient AND case_id=old.case_id AND source=old.source AND scope='case' AND revoked_at IS NULL;
  -- Preserve original addresses and transport bytes. This invalidates only
  -- uncompleted leases and changes the next claim's explicit destination.
  UPDATE moderation_delivery SET status='retry',next_attempt_at=clock_timestamp(),lease_until=NULL,generation=generation+1 WHERE (source,case_id,recipient,audience)=(old.source,old.case_id,old.recipient,old.audience) AND delivered_at IS NULL;
  RETURN 'true';
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
  INSERT INTO moderation_external_disposals(source,case_id) VALUES('challenges',(p_body->>'case_id')::uuid) ON CONFLICT DO NOTHING;
  PERFORM moderation_dispose_delivery('challenges',(p_body->>'case_id')::uuid);
  RETURN 'true';
 WHEN 'accept_delivery' THEN
  source_name:=p_body->>'source';
  PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:'||source_name||':'||(p_body->>'case_id'),0));
  IF EXISTS(SELECT 1 FROM moderation_external_disposals WHERE source=source_name AND case_id=(p_body->>'case_id')::uuid) OR (source_name='backend' AND EXISTS(SELECT 1 FROM moderation_disposals WHERE case_id=(p_body->>'case_id')::uuid)) THEN RETURN 'true'; END IF;
  IF source_name IS NULL OR source_name NOT IN ('backend','challenges') OR jsonb_typeof(p_body->'body') IS DISTINCT FROM 'object' OR (p_body->>'id') IS NULL OR (p_body->>'case_id') IS NULL OR (p_body->>'available_at') IS NULL OR p_body->>'audience' IS NULL THEN RAISE EXCEPTION 'Complete owner message required'; END IF;
  PERFORM pg_advisory_xact_lock(hashtextextended('delivery:'||source_name||':'||(p_body->>'id'),0));
  SELECT * INTO old FROM moderation_delivery WHERE moderation_delivery.source=source_name AND id=(p_body->>'id')::uuid;
  IF FOUND THEN
   IF old.case_id<>(p_body->>'case_id')::uuid OR old.recipient IS DISTINCT FROM (p_body->>'recipient')::uuid OR old.audience<>p_body->>'audience' OR old.body IS DISTINCT FROM p_body->'body' OR old.owner_available_at IS DISTINCT FROM (p_body->>'available_at')::timestamptz OR old.owner_contact_digest IS DISTINCT FROM encode(sha256(convert_to(jsonb_build_array(p_body->'contact')::text,'UTF8')),'hex') THEN RAISE EXCEPTION 'Conflicting delivery replay'; END IF;
   RETURN 'true';
  END IF;
  -- A reviewed minimization also fences a not-yet-adopted owner claim. Keep
  -- only its digest for exact replay; a later verified correction is explicit.
  contact_minimized:=coalesce((SELECT basis='reviewed_contact_minimization' FROM moderation_recipient_contacts WHERE source=source_name AND case_id=(p_body->>'case_id')::uuid AND recipient=(p_body->>'recipient')::uuid AND audience=p_body->>'audience'),
    EXISTS(SELECT 1 FROM moderation_external_minimizations WHERE source_name='challenges' AND case_id=(p_body->>'case_id')::uuid AND field=CASE p_body->>'audience' WHEN 'author' THEN 'author_contact' WHEN 'notifier' THEN 'notifier_contact' END)
    OR EXISTS(SELECT 1 FROM moderation_retention_reviews WHERE source_name='backend' AND case_id=(p_body->>'case_id')::uuid AND record->>'action'='minimize' AND record->'unnecessary_fields' ? CASE p_body->>'audience' WHEN 'author' THEN 'author_contact' WHEN 'notifier' THEN 'notifier_contact' END));
  INSERT INTO moderation_delivery(source,id,case_id,recipient,audience,body,contact,owner_contact,owner_contact_digest,owner_available_at)
   VALUES(source_name,(p_body->>'id')::uuid,(p_body->>'case_id')::uuid,(p_body->>'recipient')::uuid,p_body->>'audience',p_body->'body',CASE WHEN contact_minimized THEN NULL WHEN p_body->>'audience'='author' THEN CASE WHEN EXISTS(SELECT 1 FROM users WHERE id=(p_body->>'recipient')::uuid) THEN (SELECT email FROM users WHERE id=(p_body->>'recipient')::uuid AND email_verified) ELSE p_body->>'contact' END ELSE p_body->>'contact' END,CASE WHEN contact_minimized THEN NULL ELSE p_body->>'contact' END,encode(sha256(convert_to(jsonb_build_array(p_body->'contact')::text,'UTF8')),'hex'),(p_body->>'available_at')::timestamptz);
  INSERT INTO moderation_recipient_contacts(source,case_id,recipient,audience,contact,basis)
   SELECT source_name,(p_body->>'case_id')::uuid,(p_body->>'recipient')::uuid,p_body->>'audience',(SELECT contact FROM moderation_delivery WHERE source=source_name AND id=(p_body->>'id')::uuid),CASE WHEN contact_minimized THEN 'reviewed_contact_minimization' ELSE 'owning_service_or_current_account_contact' END
   WHERE p_body->>'recipient' IS NOT NULL
   ON CONFLICT DO NOTHING;
  RETURN 'true';
 WHEN 'claim_email' THEN
  UPDATE moderation_delivery SET body=body-'recovery_link',status=CASE WHEN delivered_at IS NULL THEN 'expired' ELSE status END,lease_until=NULL WHERE audience='recovery' AND body ? 'recovery_link' AND (body->>'expires_at')::timestamptz<=clock_timestamp();
  FOR claimed_record IN SELECT * FROM moderation_delivery WHERE delivered_at IS NULL AND status NOT IN ('no_contact','expired') AND next_attempt_at<=clock_timestamp() AND (lease_until IS NULL OR lease_until<=clock_timestamp()) ORDER BY accepted_at FOR UPDATE SKIP LOCKED LIMIT 25 LOOP
   IF claimed_record.lease_until IS NOT NULL THEN
    INSERT INTO moderation_delivery_events(source,message_id,generation,event,evidence) VALUES(claimed_record.source,claimed_record.id,claimed_record.generation,'lease_expired_uncertain','{"meaning":"The previous worker did not confirm an outcome; transport may have accepted the message. A retry may duplicate it."}');
   END IF;
   UPDATE moderation_delivery SET generation=generation+1,attempts=attempts+1,lease_until=clock_timestamp()+interval '2 minutes',status='in_flight' WHERE source=claimed_record.source AND id=claimed_record.id;
   INSERT INTO moderation_delivery_events(source,message_id,generation,event,evidence) VALUES(claimed_record.source,claimed_record.id,claimed_record.generation+1,'claimed',jsonb_build_object('destination_digest',encode(sha256(convert_to(jsonb_build_array(CASE WHEN claimed_record.audience='recovery' THEN claimed_record.contact ELSE (SELECT c.contact FROM moderation_recipient_contacts c WHERE (c.source,c.case_id,c.recipient,c.audience)=(claimed_record.source,claimed_record.case_id,claimed_record.recipient,claimed_record.audience)) END)::text,'UTF8')),'hex')));
   claimed_messages:=claimed_messages||(SELECT jsonb_build_array(to_jsonb(m)||jsonb_build_object('contact',CASE WHEN m.audience='recovery' THEN m.contact ELSE (SELECT c.contact FROM moderation_recipient_contacts c WHERE (c.source,c.case_id,c.recipient,c.audience)=(m.source,m.case_id,m.recipient,m.audience)) END)) FROM moderation_delivery m WHERE m.source=claimed_record.source AND m.id=claimed_record.id);
  END LOOP;
  RETURN claimed_messages;
 WHEN 'ack_email' THEN
  IF p_body->>'status' IS NULL OR p_body->>'status' NOT IN ('transport_accepted','retry','uncertain','no_contact') THEN RAISE EXCEPTION 'Actual transport outcome required'; END IF;
  SELECT * INTO old FROM moderation_delivery WHERE source=p_body->>'source' AND id=(p_body->>'id')::uuid FOR UPDATE;
  IF NOT FOUND OR old.generation IS DISTINCT FROM (p_body->>'generation')::bigint THEN RETURN 'false'; END IF;
  IF old.lease_until IS NULL THEN RETURN to_jsonb(old.status=p_body->>'status'); END IF;
  IF old.lease_until<=clock_timestamp() THEN RETURN 'false'; END IF;
  UPDATE moderation_delivery SET delivered_at=CASE WHEN p_body->>'status'='transport_accepted' THEN clock_timestamp() END,
   status=p_body->>'status',lease_until=NULL,next_attempt_at=clock_timestamp()+make_interval(secs=>least(86400,30*(2^least(attempts,11))::integer))
  WHERE source=old.source AND id=old.id;
  INSERT INTO moderation_delivery_events(source,message_id,generation,event,evidence) VALUES(old.source,old.id,old.generation,p_body->>'status',jsonb_build_object('meaning',CASE p_body->>'status' WHEN 'transport_accepted' THEN 'SMTP transport acknowledged acceptance; recipient information or reading is not established.' WHEN 'uncertain' THEN 'No reliable transport outcome; retry may duplicate a previously accepted message.' WHEN 'no_contact' THEN 'No usable recipient contact; human recovery work remains necessary.' ELSE 'Retry scheduled; no recipient information established.' END));
  RETURN 'true';
 ELSE RAISE EXCEPTION 'Unsupported moderation operation';
 END CASE;
END $$;

CREATE FUNCTION moderation_disposal_adapter(p_case uuid) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:backend:'||p_case,0));
 IF EXISTS(SELECT 1 FROM moderation_handling WHERE case_id=p_case AND status NOT IN ('completed','not_applicable')) THEN RAISE EXCEPTION 'Unresolved commercial/rights handling prevents disposal'; END IF;
 PERFORM set_config('academy.moderation_disposal','authorized',true);
 DELETE FROM moderation_handling_events WHERE case_id=p_case;
 PERFORM set_config('academy.moderation_disposal','',true);
 DELETE FROM moderation_handling WHERE case_id=p_case;
 PERFORM moderation_dispose_delivery('backend',p_case);
END $$;

CREATE FUNCTION moderation_dispose_delivery(p_source text,p_case uuid) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 PERFORM set_config('academy.moderation_disposal','authorized',true);
 DELETE FROM moderation_delivery_events e USING moderation_delivery m WHERE (e.source,e.message_id)=(m.source,m.id) AND m.case_id=p_case AND (m.source=p_source OR (m.audience='recovery' AND m.body->>'case_source'=p_source));
 DELETE FROM moderation_delivery WHERE case_id=p_case AND (source=p_source OR (audience='recovery' AND body->>'case_source'=p_source));
 DELETE FROM moderation_capabilities WHERE case_id=p_case AND source=p_source;
 DELETE FROM moderation_recipient_contacts WHERE case_id=p_case AND source=p_source;
 PERFORM set_config('academy.moderation_disposal','',true);
END $$;
CREATE TABLE moderation_external_minimizations(id uuid PRIMARY KEY,case_id uuid NOT NULL,field text NOT NULL,received_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE FUNCTION moderation_minimize_contacts(p_source text,p_case uuid,p_audience text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:'||p_source||':'||p_case,0));
 UPDATE moderation_delivery SET contact=NULL,owner_contact=NULL WHERE source=p_source AND case_id=p_case AND audience=p_audience;
 UPDATE moderation_delivery SET body=body-'recovery_link',contact=NULL,status='expired',lease_until=NULL,generation=generation+1 WHERE audience='recovery' AND case_id=p_case AND body->>'case_source'=p_source AND recipient IN (SELECT recipient FROM moderation_recipient_contacts WHERE source=p_source AND case_id=p_case AND audience=p_audience);
 UPDATE moderation_capabilities SET revoked_at=clock_timestamp() WHERE source=p_source AND case_id=p_case AND scope='case' AND subject IN (SELECT recipient FROM moderation_recipient_contacts WHERE source=p_source AND case_id=p_case AND audience=p_audience);
 UPDATE moderation_recipient_contacts SET contact=NULL,basis='reviewed_contact_minimization' WHERE source=p_source AND case_id=p_case AND audience=p_audience;
END $$;
CREATE FUNCTION moderation_minimize_adapter(p_case uuid,p_field text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF p_field IN ('notifier_contact','author_contact') THEN PERFORM moderation_minimize_contacts('backend',p_case,CASE p_field WHEN 'author_contact' THEN 'author' ELSE 'notifier' END); END IF;
END $$;

CREATE FUNCTION moderation_pending_work(p_case uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM moderation_handling WHERE case_id=p_case AND status NOT IN ('completed','not_applicable'))
$$;
