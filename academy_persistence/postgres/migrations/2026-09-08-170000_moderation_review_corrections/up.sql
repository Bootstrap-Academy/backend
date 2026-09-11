-- Review-1 corrections. Existing decisions/receipts remain immutable.
CREATE TABLE moderation_notifier_cleanup (subject uuid PRIMARY KEY,received_at timestamptz NOT NULL DEFAULT clock_timestamp());
ALTER TABLE moderation_cases ADD COLUMN work_review_at timestamptz;
ALTER TABLE moderation_cases ADD COLUMN notice_review_at timestamptz;
UPDATE moderation_cases SET work_review_at=review_due_at;

CREATE FUNCTION moderation_update_review_due(p_case uuid) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 UPDATE moderation_cases c SET review_due_at=(SELECT min(due) FROM (
  SELECT c.work_review_at AS due
  UNION ALL SELECT min(received_at)+interval '14 days' FROM moderation_complaints WHERE case_id=c.id AND outcome_decision IS NULL
  UNION ALL SELECT min(e.recorded_at) FROM moderation_escalations e WHERE e.case_id=c.id AND e.record->>'kind'<>'work_resolution'
   AND NOT EXISTS(SELECT 1 FROM moderation_escalations r WHERE r.case_id=c.id AND r.record->>'kind'='work_resolution' AND r.record->>'resolves'=e.id::text)
  UNION ALL SELECT clock_timestamp() WHERE moderation_pending_work(c.id)
  UNION ALL SELECT coalesce(c.notice_review_at,min(available_at)) FROM moderation_messages WHERE case_id=c.id AND decision_id IS NOT NULL AND informed_at IS NULL HAVING count(*)>0
  UNION ALL SELECT min((r.record->>'review_at')::timestamptz) FROM moderation_retention_reviews r WHERE r.case_id=c.id AND r.record->>'action'='retain'
   AND NOT EXISTS(SELECT 1 FROM moderation_retention_reviews released WHERE released.case_id=c.id AND released.record->>'action'='release_retention' AND released.record->>'retention_id'=r.id::text)
 ) outstanding) WHERE c.id=p_case;
END $$;
CREATE OR REPLACE FUNCTION moderation_open(p_id uuid,p_actor uuid,p_kind text,p_target uuid,p_subject uuid,p_source text,p_notifier uuid,p_evidence jsonb)
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
    INSERT INTO moderation_cases(id,target_kind,target_id,subject,source,created_by,notifier,private_evidence,work_review_at)
      VALUES(p_id,p_kind,p_target,p_subject,p_source,p_actor,p_notifier,p_evidence,clock_timestamp()+interval '7 days');
    IF p_notifier IS NOT NULL OR p_evidence ? 'notifier_contact' THEN
      INSERT INTO moderation_messages(id,case_id,recipient,audience,body) VALUES(gen_random_uuid(),p_id,p_notifier,'notifier',
        jsonb_build_object('status','received','text','Deine Meldung ist eingegangen. Ein Mensch prüft sie.','automation','Der Eingang wurde automatisch gespeichert. Noch keine menschliche Entscheidung.',
        'redress','Nach Information über unsere Entscheidung kannst du mindestens sechs Kalendermonate kostenlos eine menschliche Überprüfung über diesen Vorgang oder hallo@bootstrap.academy verlangen. Auch spätere Beschwerden werden zur menschlichen Prüfung angenommen. Andere Rechtsbehelfe bleiben unberührt.'));
    END IF;
    PERFORM moderation_update_review_due(p_id);
    RETURN p_id;
END $$;

CREATE OR REPLACE FUNCTION moderation_decide(p_actor uuid,p_request jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c moderation_cases; prior moderation_decisions; d uuid:=gen_random_uuid(); k uuid:=(p_request->>'request_key')::uuid;
    outcome text:=p_request->>'outcome'; effect text; deadline timestamptz; available timestamptz:=clock_timestamp();
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
      'article23_applicability',coalesce(p_request->>'article23_applicability','undetermined'),'article23_basis',p_request->>'article23_basis','misconduct_facts',p_request->>'misconduct_facts','proportionality',p_request->>'proportionality','duration_policy',p_request->>'duration_policy','previous_unrescinded_sanctions',previous_sanctions,'hearing',p_request->>'hearing','human_review',p_request->'human_review','review_assessment',p_request->>'review_assessment','historical_only',historical,'reviewed_decision_id',CASE WHEN outcome='uphold' THEN reviewed.id END,'upheld_measure',CASE WHEN outcome='uphold' THEN effect END,'effective',moderation_effect(c.target_kind,c.target_id)-'holds');
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

CREATE OR REPLACE FUNCTION moderation_complain(p_user uuid,p_request jsonb) RETURNS uuid LANGUAGE plpgsql AS $$
DECLARE existing moderation_complaints; m moderation_messages; new_id uuid:=(p_request->>'id')::uuid;
BEGIN
    IF new_id IS NULL OR p_user IS NULL OR (p_request->>'decision_id') IS NULL OR length(trim(coalesce(p_request->>'text','')))<1 THEN RAISE EXCEPTION 'Complete complaint identity and text required'; END IF;
    IF jsonb_typeof(p_request) IS DISTINCT FROM 'object' OR jsonb_typeof(p_request->'id') IS DISTINCT FROM 'string' OR jsonb_typeof(p_request->'decision_id') IS DISTINCT FROM 'string' OR jsonb_typeof(p_request->'text') IS DISTINCT FROM 'string' OR length(p_request->>'text')>16000 THEN RAISE EXCEPTION 'Complaint requires UUID strings and exact string text'; END IF;
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
    UPDATE moderation_cases SET closed_at=NULL WHERE moderation_cases.id=m.case_id;
    INSERT INTO moderation_messages(id,case_id,recipient,audience,body) VALUES(gen_random_uuid(),m.case_id,p_user,m.audience,jsonb_build_object('status','complaint_received','complaint_id',new_id,'decision_id',m.decision_id,'text','Deine Beschwerde ist eingegangen und wartet auf menschliche Überprüfung.','automation','Nur Eingangsbestätigung; keine automatische Beschwerdeentscheidung.'));
    PERFORM moderation_update_review_due(m.case_id);
    RETURN new_id;
END $$;

CREATE OR REPLACE FUNCTION moderation_escalate(p_actor uuid,p_request jsonb) RETURNS uuid LANGUAGE plpgsql AS $$
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
 UPDATE moderation_cases SET closed_at=NULL WHERE id=(p_request->>'case_id')::uuid;
 PERFORM moderation_update_review_due((p_request->>'case_id')::uuid);
 RETURN new_id;
END $$;

CREATE OR REPLACE FUNCTION moderation_withdraw_target(p_kind text,p_target uuid) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 PERFORM pg_advisory_xact_lock(hashtextextended('moderation:'||p_kind||':'||p_target,0));
 UPDATE moderation_targets SET withdrawn=true WHERE kind=p_kind AND id=p_target;
 -- Withdrawal ends ordinary application, without rescinding the historical
 -- merits or pretending that an independent binding order was revoked.
 UPDATE moderation_holds SET active=false WHERE target_kind=p_kind AND target_id=p_target AND NOT authority_order;
 UPDATE moderation_cases SET disposition_at=clock_timestamp(),closed_at=NULL,work_review_at=clock_timestamp() WHERE target_kind=p_kind AND target_id=p_target;
 PERFORM moderation_update_review_due(id) FROM moderation_cases WHERE target_kind=p_kind AND target_id=p_target;
END $$;

CREATE OR REPLACE FUNCTION moderation_legacy_statements() RETURNS void LANGUAGE plpgsql AS $$
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
  UPDATE moderation_cases SET latest_decision=d,revision=1,work_review_at=clock_timestamp() WHERE id=c.id;
  INSERT INTO moderation_messages(id,case_id,decision_id,recipient,audience,body) VALUES(gen_random_uuid(),c.id,d,c.subject,'author',statement);
  IF c.notifier IS NOT NULL THEN INSERT INTO moderation_messages(id,case_id,decision_id,recipient,audience,body) VALUES(gen_random_uuid(),c.id,d,c.notifier,'notifier',jsonb_build_object('decision_id',d,'case_id',c.id,'outcome','legacy_observed','text','Deine frühere Meldung wurde übernommen. Ein früheres Ergebnis und eine frühere Benachrichtigung sind hier nicht nachgewiesen. Menschliche Prüfung steht aus.','automation',statement->'automation','redress',statement->'redress')); END IF;
  PERFORM moderation_update_review_due(c.id);
 END LOOP;
END $$;

CREATE OR REPLACE FUNCTION moderation_opened(p_user uuid,p_id uuid) RETURNS boolean LANGUAGE plpgsql AS $$
BEGIN
 UPDATE moderation_messages SET first_opened_at=coalesce(first_opened_at,clock_timestamp()),
  informed_at=coalesce(informed_at,clock_timestamp()),notification_evidence=coalesce(notification_evidence,jsonb_build_object('basis','recipient_opened','recorded_at',clock_timestamp())),
  complaint_until=coalesce(complaint_until,moderation_review_minimum(clock_timestamp()))
 WHERE id=p_id AND recipient=p_user AND available_at<=clock_timestamp();
 IF NOT FOUND THEN RETURN false; END IF;
 PERFORM moderation_update_review_due((SELECT case_id FROM moderation_messages WHERE id=p_id));
 RETURN true;
END $$;

CREATE OR REPLACE FUNCTION moderation_retention(p_actor uuid,p_body jsonb) RETURNS boolean LANGUAGE plpgsql AS $$
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
   IF field IN ('notifier_contact','author_contact') AND c.closed_at IS NULL THEN
    IF length(trim(coalesce(p_body->>'contact_necessity_assessment','')))<3 OR length(trim(coalesce(p_body->>'remaining_remedy_access','')))<3
     OR p_body->'unnotified_rights_preserved' IS DISTINCT FROM 'true'::jsonb OR (p_body->>'review_at')::timestamptz IS NULL OR (p_body->>'review_at')::timestamptz<=clock_timestamp()
      THEN RAISE EXCEPTION 'Assess unnecessary contact, preserved remedy access and a future notice review without inventing information'; END IF;
    UPDATE moderation_cases SET notice_review_at=(p_body->>'review_at')::timestamptz WHERE id=c.id;
   END IF;
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
 PERFORM moderation_update_review_due(c.id);
 RETURN true;
END $$;

CREATE OR REPLACE FUNCTION moderation_maintenance() RETURNS integer LANGUAGE plpgsql AS $$
DECLARE c moderation_cases; h moderation_holds; previous moderation_decisions; cmd jsonb; n integer:=0;
BEGIN
 FOR h IN SELECT * FROM moderation_holds WHERE active AND ends_at<=clock_timestamp() ORDER BY target_kind,target_id,case_id LIMIT 1 LOOP
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
 -- Each expiry transaction owns only one target. Closure runs without a target lock.
 IF n>0 THEN RETURN n; END IF;
 -- Notifier cleanup uses its own transaction, after account erasure committed.
 FOR c IN SELECT mc.* FROM moderation_cases mc JOIN moderation_notifier_cleanup w ON w.subject=mc.notifier
   WHERE mc.private_evidence ? 'notifier_contact' ORDER BY w.received_at,mc.id FOR UPDATE OF mc SKIP LOCKED LIMIT 1 LOOP
  UPDATE moderation_cases SET private_evidence=private_evidence-'notifier_contact' WHERE id=c.id;
  RETURN 1;
 END LOOP;
 DELETE FROM moderation_notifier_cleanup w WHERE NOT EXISTS(SELECT 1 FROM moderation_cases pending_case WHERE pending_case.notifier=w.subject AND pending_case.private_evidence ? 'notifier_contact');
 UPDATE moderation_cases closing_case SET closed_at=clock_timestamp(),review_due_at=NULL,work_review_at=NULL
 WHERE closed_at IS NULL AND disposition_at IS NOT NULL AND NOT EXISTS(SELECT 1 FROM moderation_holds WHERE case_id=closing_case.id AND active)
 AND NOT EXISTS(SELECT 1 FROM moderation_complaints WHERE case_id=closing_case.id AND outcome_decision IS NULL) AND NOT moderation_pending_escalation(closing_case.id) AND NOT moderation_pending_work(closing_case.id)
 AND NOT EXISTS(SELECT 1 FROM moderation_messages WHERE case_id=closing_case.id AND decision_id IS NOT NULL AND (informed_at IS NULL OR complaint_until>clock_timestamp()));
 RETURN n;
END $$;

CREATE FUNCTION moderation_message_review_due() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM moderation_update_review_due(NEW.case_id);
 RETURN NEW;
END $$;
CREATE TRIGGER moderation_message_review_due AFTER INSERT ON moderation_messages FOR EACH ROW EXECUTE FUNCTION moderation_message_review_due();
SELECT moderation_update_review_due(id) FROM moderation_cases WHERE closed_at IS NULL;

CREATE OR REPLACE FUNCTION moderation_erase(p_user uuid) RETURNS void LANGUAGE plpgsql AS $$
DECLARE target moderation_targets;
BEGIN
    FOR target IN SELECT * FROM moderation_targets WHERE subject=p_user ORDER BY kind,id LOOP
      PERFORM moderation_withdraw_target(target.kind,target.id);
    END LOOP;
    INSERT INTO moderation_erasure_events(subject) VALUES(p_user) ON CONFLICT DO NOTHING;
    UPDATE moderation_targets SET withdrawn=true WHERE subject=p_user;
    -- Statements/necessary dispute evidence have their own review/retention lifecycle.
    INSERT INTO moderation_notifier_cleanup(subject) VALUES(p_user) ON CONFLICT DO NOTHING;
END $$;


CREATE TABLE moderation_retained_record_owners (
 subject uuid NOT NULL,kind text NOT NULL CHECK(kind IN ('financial_document','contract_declaration')),record_id text NOT NULL,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(kind,record_id),UNIQUE(subject,kind,record_id)
);
ALTER TABLE moderation_erasure_events ADD COLUMN retained_owner_inventory boolean NOT NULL DEFAULT false;
CREATE TRIGGER moderation_retained_record_owners_immutable BEFORE UPDATE OR DELETE ON moderation_retained_record_owners FOR EACH ROW EXECUTE FUNCTION moderation_immutable();


-- Backend-specific authority and transport corrections.

ALTER TABLE moderation_recipient_contacts ADD COLUMN authority_kind text NOT NULL DEFAULT 'unknown'
 CHECK(authority_kind IN ('unknown','verified_account','verified_case','notifier_channel','minimized'));
UPDATE moderation_recipient_contacts c SET authority_kind=CASE
 WHEN basis='reviewed_contact_minimization' THEN 'minimized'
 WHEN basis='verified_case_contact_correction' THEN 'verified_case'
 WHEN audience='notifier' AND contact IS NOT NULL THEN 'notifier_channel'
 WHEN basis='current_account_contact_verification' AND contact IS NOT NULL THEN 'verified_account'
 WHEN audience='author' AND EXISTS(SELECT 1 FROM users u WHERE u.id=c.recipient AND u.email_verified AND u.email=c.contact) THEN 'verified_account'
 ELSE 'unknown' END;
-- Unknown old addresses remain transport history, never recovery authority.
UPDATE moderation_recipient_contacts SET contact=NULL,basis='verification_not_established' WHERE authority_kind='unknown';
UPDATE moderation_capabilities p SET revoked_at=clock_timestamp() WHERE p.scope='case' AND EXISTS(
 SELECT 1 FROM moderation_recipient_contacts c WHERE (c.source,c.case_id,c.recipient)=(p.source,p.case_id,p.subject) AND c.authority_kind='unknown');

-- Minimal admitted-attempt evidence survives a concurrent case disposal until
-- actual transport outcome is known. It contains no address, body or capability.
CREATE TABLE moderation_send_attempts (
 id uuid PRIMARY KEY,source text NOT NULL,authority_source text NOT NULL,message_id uuid NOT NULL,case_id uuid NOT NULL,generation bigint NOT NULL,
 admitted_at timestamptz NOT NULL DEFAULT clock_timestamp(),lease_until timestamptz NOT NULL,
 destination_digest text NOT NULL,body_digest text NOT NULL,
 UNIQUE(source,message_id,generation)
);
CREATE TRIGGER moderation_send_attempts_immutable BEFORE UPDATE OR DELETE ON moderation_send_attempts FOR EACH ROW EXECUTE FUNCTION moderation_immutable();
CREATE TABLE moderation_send_outcomes (
 attempt_id uuid PRIMARY KEY REFERENCES moderation_send_attempts(id),status text NOT NULL CHECK(status IN ('transport_accepted','retry','uncertain','no_contact')),
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER moderation_send_outcomes_immutable BEFORE UPDATE OR DELETE ON moderation_send_outcomes FOR EACH ROW EXECUTE FUNCTION moderation_immutable();

CREATE FUNCTION moderation_admit_email(p_source text,p_id uuid,p_generation bigint) RETURNS jsonb LANGUAGE plpgsql AS $$
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
 RETURN to_jsonb(m)||jsonb_build_object('contact',destination,'attempt_id',attempt);
END $$;

CREATE FUNCTION moderation_ack_email(p_body jsonb) RETURNS boolean LANGUAGE plpgsql AS $$
DECLARE a moderation_send_attempts; previous moderation_send_outcomes; m moderation_delivery; tombstone record; archived jsonb; history jsonb; event jsonb; actual_status text;
BEGIN
 IF p_body->>'status' IS NULL OR p_body->>'status' NOT IN ('transport_accepted','retry','uncertain','no_contact') THEN RAISE EXCEPTION 'Actual transport outcome required'; END IF;
 SELECT * INTO a FROM moderation_send_attempts WHERE id=(p_body->>'attempt_id')::uuid;
 IF FOUND THEN
  PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:'||a.authority_source||':'||a.case_id,0));
  -- Disposal and acknowledgements use the same owner-case fence. Re-read after
  -- waiting; the earlier lookup is only lock routing, never surviving authority.
  SELECT * INTO a FROM moderation_send_attempts WHERE id=(p_body->>'attempt_id')::uuid;
 END IF;
 IF NOT FOUND THEN
  FOR tombstone IN SELECT 'backend' AS source,case_id FROM moderation_disposals d WHERE EXISTS(SELECT 1 FROM jsonb_array_elements(d.transport_evidence) item WHERE item->>'source'=p_body->>'source' AND item->>'attempt_id'=p_body->>'attempt_id')
   UNION ALL SELECT source,case_id FROM moderation_external_disposals d WHERE EXISTS(SELECT 1 FROM jsonb_array_elements(d.transport_evidence) item WHERE item->>'source'=p_body->>'source' AND item->>'attempt_id'=p_body->>'attempt_id') LOOP
   PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:'||tombstone.source||':'||tombstone.case_id,0));
   IF tombstone.source='backend' THEN SELECT transport_evidence INTO history FROM moderation_disposals WHERE case_id=tombstone.case_id;
   ELSE SELECT transport_evidence INTO history FROM moderation_external_disposals WHERE source=tombstone.source AND case_id=tombstone.case_id; END IF;
   SELECT item INTO archived FROM jsonb_array_elements(history) item WHERE item->>'source'=p_body->>'source' AND item->>'attempt_id'=p_body->>'attempt_id' AND item->>'message_id'=p_body->>'id' AND item->>'generation'=p_body->>'generation' LIMIT 1;
   IF FOUND THEN
    SELECT item->>'status' INTO actual_status FROM jsonb_array_elements(history) item WHERE item->>'source'=p_body->>'source' AND item->>'attempt_id'=p_body->>'attempt_id' AND item->>'status' IN ('transport_accepted','retry','uncertain','no_contact') LIMIT 1;
    IF FOUND THEN RETURN actual_status=p_body->>'status'; END IF;
    event:=jsonb_build_object('source',p_body->>'source','attempt_id',p_body->>'attempt_id','message_id',p_body->>'id','generation',p_body->'generation','status',p_body->>'status','observed_at',clock_timestamp(),'meaning','Late actual transport observation after reviewed case disposal; disposal did not recall transmission.');
    IF tombstone.source='backend' THEN UPDATE moderation_disposals SET transport_evidence=transport_evidence||jsonb_build_array(event) WHERE case_id=tombstone.case_id;
    ELSE UPDATE moderation_external_disposals SET transport_evidence=transport_evidence||jsonb_build_array(event) WHERE source=tombstone.source AND case_id=tombstone.case_id; END IF;
    RETURN true;
   END IF;
  END LOOP;
  RETURN false;
 END IF;
 IF a.source IS DISTINCT FROM p_body->>'source' OR a.message_id IS DISTINCT FROM (p_body->>'id')::uuid OR a.generation IS DISTINCT FROM (p_body->>'generation')::bigint THEN RETURN false; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('moderation-send-outcome:'||a.id,0));
 SELECT * INTO previous FROM moderation_send_outcomes WHERE attempt_id=a.id;
 IF FOUND THEN RETURN previous.status=p_body->>'status'; END IF;
 INSERT INTO moderation_send_outcomes(attempt_id,status) VALUES(a.id,p_body->>'status');
 SELECT * INTO m FROM moderation_delivery WHERE source=a.source AND id=a.message_id FOR UPDATE;
 IF FOUND AND m.generation=a.generation THEN
  UPDATE moderation_delivery SET delivered_at=CASE WHEN p_body->>'status'='transport_accepted' THEN clock_timestamp() END,status=p_body->>'status',lease_until=NULL,
   next_attempt_at=clock_timestamp()+make_interval(secs=>least(86400,30*(2^least(attempts,11))::integer)) WHERE source=m.source AND id=m.id;
  INSERT INTO moderation_delivery_events(source,message_id,generation,event,evidence) VALUES(m.source,m.id,m.generation,p_body->>'status',jsonb_build_object('attempt_id',a.id,'meaning','Actual transport observation; recipient information remains independently evidenced.'));
 END IF;
 RETURN true;
END $$;

CREATE OR REPLACE FUNCTION backend_moderation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
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
   VALUES(source_name,(p_body->>'id')::uuid,(p_body->>'case_id')::uuid,(p_body->>'recipient')::uuid,p_body->>'audience',p_body->'body',CASE WHEN contact_minimized THEN NULL WHEN p_body->>'audience'='author' THEN (SELECT email FROM users WHERE id=(p_body->>'recipient')::uuid AND email_verified) ELSE p_body->>'contact' END,CASE WHEN contact_minimized THEN NULL ELSE p_body->>'contact' END,encode(sha256(convert_to(jsonb_build_array(p_body->'contact')::text,'UTF8')),'hex'),(p_body->>'available_at')::timestamptz);
  INSERT INTO moderation_recipient_contacts(source,case_id,recipient,audience,contact,basis,authority_kind)
   SELECT source_name,(p_body->>'case_id')::uuid,(p_body->>'recipient')::uuid,p_body->>'audience',(SELECT contact FROM moderation_delivery WHERE source=source_name AND id=(p_body->>'id')::uuid),CASE WHEN contact_minimized THEN 'reviewed_contact_minimization' WHEN p_body->>'audience'='author' THEN 'current_verified_account_or_unknown' ELSE 'notifier_supplied_channel' END,CASE WHEN contact_minimized THEN 'minimized' WHEN p_body->>'audience'='notifier' THEN 'notifier_channel' WHEN EXISTS(SELECT 1 FROM users WHERE id=(p_body->>'recipient')::uuid AND email_verified) THEN 'verified_account' ELSE 'unknown' END
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
   claimed_messages:=claimed_messages||(SELECT jsonb_build_array(jsonb_build_object('source',m.source,'id',m.id,'generation',m.generation)) FROM moderation_delivery m WHERE m.source=claimed_record.source AND m.id=claimed_record.id);
  END LOOP;
  RETURN claimed_messages;
 WHEN 'admit_email' THEN RETURN moderation_admit_email(p_body->>'source',(p_body->>'id')::uuid,(p_body->>'generation')::bigint);
 WHEN 'ack_email' THEN RETURN to_jsonb(moderation_ack_email(p_body));
 ELSE RAISE EXCEPTION 'Unsupported moderation operation';
 END CASE;
END $$;

CREATE OR REPLACE FUNCTION moderation_user_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE contact_case record;
BEGIN
 IF TG_OP='DELETE' THEN
  IF current_setting('academy.moderation_erasure_subject',true) IS DISTINCT FROM OLD.id::text THEN RAISE EXCEPTION 'Account removal requires authenticated self-erasure'; END IF;
  INSERT INTO moderation_retained_record_owners(subject,kind,record_id) SELECT OLD.id,'financial_document',number FROM financial_documents WHERE user_id=OLD.id ON CONFLICT DO NOTHING;
  INSERT INTO moderation_retained_record_owners(subject,kind,record_id) SELECT OLD.id,'contract_declaration',id::text FROM contract_declarations WHERE user_id=OLD.id ON CONFLICT DO NOTHING;
  PERFORM moderation_erase(OLD.id);
  UPDATE moderation_erasure_events SET retained_owner_inventory=true WHERE subject=OLD.id;
  RETURN OLD;
 END IF;
 IF (NEW.email,NEW.email_verified) IS DISTINCT FROM (OLD.email,OLD.email_verified) THEN
  FOR contact_case IN SELECT DISTINCT source,case_id FROM moderation_recipient_contacts WHERE recipient=OLD.id AND audience='author' ORDER BY source,case_id LOOP
   PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:'||contact_case.source||':'||contact_case.case_id,0));
  END LOOP;
  UPDATE moderation_recipient_contacts SET contact=CASE WHEN NEW.email_verified THEN NEW.email END,updated_at=clock_timestamp(),basis='current_account_contact_verification',authority_kind=CASE WHEN NEW.email_verified THEN 'verified_account' ELSE 'unknown' END WHERE recipient=OLD.id AND audience='author' AND authority_kind<>'minimized';
  UPDATE moderation_capabilities SET revoked_at=clock_timestamp() WHERE subject=OLD.id AND scope='case' AND revoked_at IS NULL;
  UPDATE moderation_delivery SET status='retry',lease_until=NULL,generation=generation+1,next_attempt_at=clock_timestamp() WHERE recipient=OLD.id AND audience='author' AND delivered_at IS NULL AND EXISTS(SELECT 1 FROM moderation_recipient_contacts c WHERE (c.source,c.case_id,c.recipient,c.audience)=(moderation_delivery.source,moderation_delivery.case_id,moderation_delivery.recipient,moderation_delivery.audience) AND c.authority_kind<>'minimized');
  UPDATE moderation_delivery SET status='expired',body=body-'recovery_link',lease_until=NULL,generation=generation+1 WHERE recipient=OLD.id AND audience='recovery';
 END IF;
 IF NEW.enabled IS DISTINCT FROM OLD.enabled AND current_setting('academy.moderation_write',true) IS DISTINCT FROM 'authorized' THEN
  RAISE EXCEPTION 'Use a complete moderation decision to restrict or restore an account';
 END IF;
 RETURN NEW;
END $$;

CREATE OR REPLACE FUNCTION moderation_minimize_contacts(p_source text,p_case uuid,p_audience text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 PERFORM pg_advisory_xact_lock(hashtextextended('external-disposal:'||p_source||':'||p_case,0));
 UPDATE moderation_delivery SET contact=NULL,owner_contact=NULL,generation=generation+1,lease_until=NULL,status=CASE WHEN delivered_at IS NULL THEN 'no_contact' ELSE status END WHERE source=p_source AND case_id=p_case AND audience=p_audience;
 UPDATE moderation_delivery SET body=body-'recovery_link',contact=NULL,status='expired',lease_until=NULL,generation=generation+1 WHERE audience='recovery' AND case_id=p_case AND body->>'case_source'=p_source AND recipient IN (SELECT recipient FROM moderation_recipient_contacts WHERE source=p_source AND case_id=p_case AND audience=p_audience);
 UPDATE moderation_capabilities SET revoked_at=clock_timestamp() WHERE source=p_source AND case_id=p_case AND scope='case' AND subject IN (SELECT recipient FROM moderation_recipient_contacts WHERE source=p_source AND case_id=p_case AND audience=p_audience);
 UPDATE moderation_recipient_contacts SET contact=NULL,basis='reviewed_contact_minimization',authority_kind='minimized' WHERE source=p_source AND case_id=p_case AND audience=p_audience;
END $$;

CREATE FUNCTION moderation_retained_owner_removed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE previous_guard text:=current_setting('academy.moderation_disposal',true);
BEGIN
 PERFORM set_config('academy.moderation_disposal','authorized',true);
 DELETE FROM moderation_retained_record_owners WHERE kind=TG_ARGV[0] AND record_id=CASE WHEN TG_ARGV[0]='financial_document' THEN to_jsonb(OLD)->>'number' ELSE to_jsonb(OLD)->>'id' END;
 PERFORM set_config('academy.moderation_disposal',coalesce(previous_guard,''),true);
 RETURN OLD;
END $$;
CREATE TRIGGER moderation_financial_owner_removed AFTER DELETE ON financial_documents FOR EACH ROW EXECUTE FUNCTION moderation_retained_owner_removed('financial_document');
CREATE TRIGGER moderation_declaration_owner_removed AFTER DELETE ON contract_declarations FOR EACH ROW EXECUTE FUNCTION moderation_retained_owner_removed('contract_declaration');

ALTER TABLE moderation_disposals ADD COLUMN transport_evidence jsonb NOT NULL DEFAULT '[]';
ALTER TABLE moderation_external_disposals ADD COLUMN transport_evidence jsonb NOT NULL DEFAULT '[]';
ALTER TABLE moderation_external_disposals ADD COLUMN review_due_at timestamptz;
-- This summary is part of the already-reviewed minimal case-disposal receipt.
-- It has no recipient, address, source body, capability or destination digest.
CREATE FUNCTION moderation_archive_send_attempts(p_source text,p_case uuid) RETURNS void LANGUAGE plpgsql AS $$
DECLARE summary jsonb; old_guard text:=current_setting('academy.moderation_disposal',true);
BEGIN
 SELECT coalesce(jsonb_agg(jsonb_build_object('source',a.source,'attempt_id',a.id,'message_id',a.message_id,'generation',a.generation,'admitted_at',a.admitted_at,'lease_until',a.lease_until,
  'status',coalesce(o.status,'outcome_unknown_at_reviewed_disposal'),'observed_at',o.observed_at)),'[]'::jsonb) INTO summary
 FROM moderation_send_attempts a LEFT JOIN moderation_send_outcomes o ON o.attempt_id=a.id WHERE a.authority_source=p_source AND a.case_id=p_case;
 IF p_source='backend' THEN UPDATE moderation_disposals SET transport_evidence=transport_evidence||summary WHERE case_id=p_case;
 ELSE UPDATE moderation_external_disposals SET transport_evidence=transport_evidence||summary WHERE source=p_source AND case_id=p_case; END IF;
 PERFORM set_config('academy.moderation_disposal','authorized',true);
 DELETE FROM moderation_send_outcomes WHERE attempt_id IN(SELECT id FROM moderation_send_attempts WHERE authority_source=p_source AND case_id=p_case);
 DELETE FROM moderation_send_attempts WHERE authority_source=p_source AND case_id=p_case;
 PERFORM set_config('academy.moderation_disposal',coalesce(old_guard,''),true);
END $$;

CREATE OR REPLACE FUNCTION moderation_dispose_delivery(p_source text,p_case uuid) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 PERFORM moderation_archive_send_attempts(p_source,p_case);
 PERFORM set_config('academy.moderation_disposal','authorized',true);
 DELETE FROM moderation_delivery_events e USING moderation_delivery m WHERE (e.source,e.message_id)=(m.source,m.id) AND m.case_id=p_case AND (m.source=p_source OR (m.audience='recovery' AND m.body->>'case_source'=p_source));
 DELETE FROM moderation_delivery WHERE case_id=p_case AND (source=p_source OR (audience='recovery' AND body->>'case_source'=p_source));
 DELETE FROM moderation_capabilities WHERE case_id=p_case AND source=p_source;
 DELETE FROM moderation_recipient_contacts WHERE case_id=p_case AND source=p_source;
 PERFORM set_config('academy.moderation_disposal','',true);
END $$;
