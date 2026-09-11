-- Add a fixed staff read without changing the existing queues or preservation evidence.
CREATE FUNCTION commercial_retention_page_keys(p_value jsonb,p_keys text[]) RETURNS boolean
LANGUAGE sql IMMUTABLE AS $$
 SELECT CASE WHEN jsonb_typeof(p_value)='object' THEN
  (SELECT count(*)=cardinality(p_keys) FROM jsonb_object_keys(p_value)) AND p_value ?& p_keys
 ELSE false END;
$$;

-- The staff observation is deliberately outside this STABLE direct projection.
-- Each selected family obtains rows, lookahead and cursor from one local query.
CREATE FUNCTION commercial_retention_page_projection(p_family text,p_limit integer,p_after jsonb)
RETURNS jsonb LANGUAGE plpgsql STABLE SET TimeZone='UTC' SET DateStyle='ISO,YMD' AS $$
BEGIN
 IF p_family='statements' THEN
  RETURN (
   WITH selected AS MATERIALIZED (
    SELECT r.review_due_at AS at,r.number AS number,jsonb_build_object('number',r.number,'review_due_at',r.review_due_at::text,'authorized',r.authorized,'assessment_json',r.assessment::text,'historical_staff_assertion',d.settled_at::text,'issued_at',d.issued_at::text) AS value,
     jsonb_build_object('protocol',1,'family',p_family,'after',jsonb_build_object('at',r.review_due_at::text,'number',r.number)) AS cursor
    FROM commercial_statement_disposal_reviews r JOIN financial_documents d ON d.number=r.number
    WHERE (true) AND (p_after IS NULL OR (r.review_due_at,r.number COLLATE "C")>((p_after->>'at')::timestamptz,(p_after->>'number') COLLATE "C"))
    ORDER BY r.review_due_at,r.number COLLATE "C" LIMIT p_limit+1
   ), numbered AS (
    SELECT value,cursor,row_number() OVER (ORDER BY at,number COLLATE "C") AS ordinal FROM selected
   )
   SELECT jsonb_build_object('protocol',1,'family',p_family,'limit',p_limit,
    'observed_at',statement_timestamp()::text,'semantics','live_queue',
    'rows',coalesce(jsonb_agg(value ORDER BY ordinal) FILTER(WHERE ordinal<=p_limit),'[]'::jsonb),
    'next_cursor',CASE WHEN count(*)>p_limit THEN
     (jsonb_agg(cursor ORDER BY ordinal DESC) FILTER(WHERE ordinal<=p_limit))->0 ELSE NULL END,
    'exhausted',count(*)<=p_limit)
   FROM numbered
  );
 END IF;
 IF p_family='archives' THEN
  RETURN (
   WITH selected AS MATERIALIZED (
    SELECT r.review_due_at AS at,r.number AS number,r.kind AS kind,jsonb_build_object('number',r.number,'kind',r.kind,'source',r.source,'recorded_at',r.recorded_at::text,'review_due_at',r.review_due_at::text,'disposal_authorized',r.disposal_authorized,'assessment_json',r.assessment::text,'disposal_started_at',r.disposal_started_at::text,'file_removed_at',r.file_removed_at::text) AS value,
     jsonb_build_object('protocol',1,'family',p_family,'after',jsonb_build_object('at',r.review_due_at::text,'number',r.number,'kind',r.kind)) AS cursor
    FROM commercial_archive_work r
    WHERE (r.file_removed_at IS NULL) AND (p_after IS NULL OR (r.review_due_at,r.number COLLATE "C",r.kind COLLATE "C")>((p_after->>'at')::timestamptz,(p_after->>'number') COLLATE "C",(p_after->>'kind') COLLATE "C"))
    ORDER BY r.review_due_at,r.number COLLATE "C",r.kind COLLATE "C" LIMIT p_limit+1
   ), numbered AS (
    SELECT value,cursor,row_number() OVER (ORDER BY at,number COLLATE "C",kind COLLATE "C") AS ordinal FROM selected
   )
   SELECT jsonb_build_object('protocol',1,'family',p_family,'limit',p_limit,
    'observed_at',statement_timestamp()::text,'semantics','live_queue',
    'rows',coalesce(jsonb_agg(value ORDER BY ordinal) FILTER(WHERE ordinal<=p_limit),'[]'::jsonb),
    'next_cursor',CASE WHEN count(*)>p_limit THEN
     (jsonb_agg(cursor ORDER BY ordinal DESC) FILTER(WHERE ordinal<=p_limit))->0 ELSE NULL END,
    'exhausted',count(*)<=p_limit)
   FROM numbered
  );
 END IF;
 IF p_family='retained_owner_associations' THEN
  RETURN (
   WITH selected AS MATERIALIZED (
    SELECT r.review_due_at AS at,r.number AS number,r.kind AS kind,r.subject AS subject,jsonb_build_object('number',r.number,'kind',r.kind,'subject',r.subject,'observed_at',r.observed_at::text,'review_due_at',r.review_due_at::text,'source',r.source) AS value,
     jsonb_build_object('protocol',1,'family',p_family,'after',jsonb_build_object('at',r.review_due_at::text,'number',r.number,'kind',r.kind,'subject',r.subject)) AS cursor
    FROM commercial_retention_owners r
    WHERE (true) AND (p_after IS NULL OR (r.review_due_at,r.number COLLATE "C",r.kind COLLATE "C",r.subject)>((p_after->>'at')::timestamptz,(p_after->>'number') COLLATE "C",(p_after->>'kind') COLLATE "C",(p_after->>'subject')::uuid))
    ORDER BY r.review_due_at,r.number COLLATE "C",r.kind COLLATE "C",r.subject LIMIT p_limit+1
   ), numbered AS (
    SELECT value,cursor,row_number() OVER (ORDER BY at,number COLLATE "C",kind COLLATE "C",subject) AS ordinal FROM selected
   )
   SELECT jsonb_build_object('protocol',1,'family',p_family,'limit',p_limit,
    'observed_at',statement_timestamp()::text,'semantics','live_queue',
    'rows',coalesce(jsonb_agg(value ORDER BY ordinal) FILTER(WHERE ordinal<=p_limit),'[]'::jsonb),
    'next_cursor',CASE WHEN count(*)>p_limit THEN
     (jsonb_agg(cursor ORDER BY ordinal DESC) FILTER(WHERE ordinal<=p_limit))->0 ELSE NULL END,
    'exhausted',count(*)<=p_limit)
   FROM numbered
  );
 END IF;
 IF p_family='invoice_identity_reviews' THEN
  RETURN (
   WITH selected AS MATERIALIZED (
    SELECT r.observed_at AS at,r.number AS number,r.reason AS reason,r.source_key AS source_key,jsonb_build_object('number',r.number,'reason',r.reason,'source_key',r.source_key,'observed_at',r.observed_at::text,'disposition',r.disposition,'evidence_json',r.evidence::text) AS value,
     jsonb_build_object('protocol',1,'family',p_family,'after',jsonb_build_object('at',r.observed_at::text,'number',r.number,'reason',r.reason,'source_key',r.source_key)) AS cursor
    FROM commercial_invoice_identity_reviews r
    WHERE (true) AND (p_after IS NULL OR (r.observed_at,r.number COLLATE "C",r.reason COLLATE "C",r.source_key COLLATE "C")>((p_after->>'at')::timestamptz,(p_after->>'number') COLLATE "C",(p_after->>'reason') COLLATE "C",(p_after->>'source_key') COLLATE "C"))
    ORDER BY r.observed_at,r.number COLLATE "C",r.reason COLLATE "C",r.source_key COLLATE "C" LIMIT p_limit+1
   ), numbered AS (
    SELECT value,cursor,row_number() OVER (ORDER BY at,number COLLATE "C",reason COLLATE "C",source_key COLLATE "C") AS ordinal FROM selected
   )
   SELECT jsonb_build_object('protocol',1,'family',p_family,'limit',p_limit,
    'observed_at',statement_timestamp()::text,'semantics','live_queue',
    'rows',coalesce(jsonb_agg(value ORDER BY ordinal) FILTER(WHERE ordinal<=p_limit),'[]'::jsonb),
    'next_cursor',CASE WHEN count(*)>p_limit THEN
     (jsonb_agg(cursor ORDER BY ordinal DESC) FILTER(WHERE ordinal<=p_limit))->0 ELSE NULL END,
    'exhausted',count(*)<=p_limit)
   FROM numbered
  );
 END IF;
 IF p_family='unqualified_invoice_owner_observations' THEN
  RETURN (
   WITH selected AS MATERIALIZED (
    SELECT r.observed_at AS at,r.number AS number,r.subject AS subject,r.basis AS basis,r.evidence_hash AS evidence_hash,jsonb_build_object('number',r.number,'subject',r.subject,'basis',r.basis,'evidence_hash',r.evidence_hash,'evidence_json',r.evidence::text,'qualified',r.qualified,'observed_at',r.observed_at::text) AS value,
     jsonb_build_object('protocol',1,'family',p_family,'after',jsonb_build_object('at',r.observed_at::text,'number',r.number,'subject',r.subject,'basis',r.basis,'evidence_hash',r.evidence_hash)) AS cursor
    FROM commercial_invoice_owner_observations r
    WHERE (NOT r.qualified) AND (p_after IS NULL OR (r.observed_at,r.number COLLATE "C",r.subject,r.basis COLLATE "C",r.evidence_hash COLLATE "C")>((p_after->>'at')::timestamptz,(p_after->>'number') COLLATE "C",(p_after->>'subject')::uuid,(p_after->>'basis') COLLATE "C",(p_after->>'evidence_hash') COLLATE "C"))
    ORDER BY r.observed_at,r.number COLLATE "C",r.subject,r.basis COLLATE "C",r.evidence_hash COLLATE "C" LIMIT p_limit+1
   ), numbered AS (
    SELECT value,cursor,row_number() OVER (ORDER BY at,number COLLATE "C",subject,basis COLLATE "C",evidence_hash COLLATE "C") AS ordinal FROM selected
   )
   SELECT jsonb_build_object('protocol',1,'family',p_family,'limit',p_limit,
    'observed_at',statement_timestamp()::text,'semantics','live_queue',
    'rows',coalesce(jsonb_agg(value ORDER BY ordinal) FILTER(WHERE ordinal<=p_limit),'[]'::jsonb),
    'next_cursor',CASE WHEN count(*)>p_limit THEN
     (jsonb_agg(cursor ORDER BY ordinal DESC) FILTER(WHERE ordinal<=p_limit))->0 ELSE NULL END,
    'exhausted',count(*)<=p_limit)
   FROM numbered
  );
 END IF;
 RAISE EXCEPTION USING ERRCODE='22000', MESSAGE='Unavailable retention page family';
END $$;

CREATE FUNCTION commercial_retention_page(p_actor uuid,p_body jsonb) RETURNS jsonb
LANGUAGE plpgsql SET TimeZone='UTC' SET DateStyle='ISO,YMD' AS $$
DECLARE family text; page_limit integer; cursor jsonb; after_key jsonb; keys text[];
BEGIN
 IF NOT coalesce(commercial_retention_page_keys(p_body,ARRAY['family','limit','cursor','_staff_session','_staff_refresh_hash']),false) THEN
  RETURN jsonb_build_object('kind','malformed');
 END IF;
 IF jsonb_typeof(p_body->'family')<>'string' OR jsonb_typeof(p_body->'limit')<>'number'
  OR (p_body->>'limit') !~ '^[0-9]+$'
  OR jsonb_typeof(p_body->'_staff_session')<>'string'
  OR jsonb_typeof(p_body->'_staff_refresh_hash')<>'string' THEN
  RETURN jsonb_build_object('kind','malformed');
 END IF;
 IF (p_body->>'limit')::numeric NOT BETWEEN 1 AND 100 THEN RETURN jsonb_build_object('kind','malformed'); END IF;
 family:=p_body->>'family'; page_limit:=(p_body->>'limit')::integer;
 CASE family
 WHEN 'statements' THEN keys:=ARRAY['at','number'];
 WHEN 'archives' THEN keys:=ARRAY['at','number','kind'];
 WHEN 'retained_owner_associations' THEN keys:=ARRAY['at','number','kind','subject'];
 WHEN 'invoice_identity_reviews' THEN keys:=ARRAY['at','number','reason','source_key'];
 WHEN 'unqualified_invoice_owner_observations' THEN keys:=ARRAY['at','number','subject','basis','evidence_hash'];
 ELSE RETURN jsonb_build_object('kind','malformed');
 END CASE;
 cursor:=p_body->'cursor';
 IF cursor<>'null'::jsonb THEN
  IF NOT coalesce(commercial_retention_page_keys(cursor,ARRAY['protocol','family','after']),false)
   OR cursor->'protocol'<>'1'::jsonb OR jsonb_typeof(cursor->'protocol')<>'number'
   OR cursor->>'protocol'<>'1' OR jsonb_typeof(cursor->'family') IS DISTINCT FROM 'string'
   OR (cursor->>'family') IS DISTINCT FROM family THEN RETURN jsonb_build_object('kind','malformed'); END IF;
  after_key:=cursor->'after';
  IF NOT coalesce(commercial_retention_page_keys(after_key,keys),false)
   OR EXISTS(SELECT 1 FROM unnest(keys) k WHERE jsonb_typeof(after_key->k)<>'string') THEN
   RETURN jsonb_build_object('kind','malformed');
  END IF;
 END IF;
 -- Only supplied credential/cursor casts are normalized here. Staff and data
 -- queries remain outside this handler so failures cannot look like empty pages.
 BEGIN
  IF (p_body->>'_staff_session')::uuid::text<>p_body->>'_staff_session' THEN
   RETURN jsonb_build_object('kind','malformed');
  END IF;
  IF after_key IS NOT NULL THEN
   IF (after_key->>'at')::timestamptz::text<>after_key->>'at' THEN RETURN jsonb_build_object('kind','malformed'); END IF;
   IF after_key ? 'subject' AND (after_key->>'subject')::uuid::text<>after_key->>'subject' THEN
    RETURN jsonb_build_object('kind','malformed');
   END IF;
  END IF;
 EXCEPTION WHEN SQLSTATE '22P02' OR SQLSTATE '22007' OR SQLSTATE '22008' OR SQLSTATE '22009' OR SQLSTATE '22003' THEN
  RETURN jsonb_build_object('kind','malformed');
 END;
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Current administrator MFA authority required'; END IF;
 RETURN jsonb_build_object('kind','page','value',commercial_retention_page_projection(family,page_limit,after_key));
END $$;

ALTER FUNCTION commercial_operation(text,uuid,jsonb) RENAME TO commercial_operation_before_retention_page;
CREATE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation='admin_retention_page' THEN RETURN commercial_retention_page(p_actor,p_body); END IF;
 RETURN commercial_operation_before_retention_page(p_operation,p_actor,p_body);
END $$;
