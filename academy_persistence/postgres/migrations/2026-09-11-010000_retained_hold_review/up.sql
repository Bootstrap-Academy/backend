-- Existing holds only: no release, new evidence capture or retention-policy decision.
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['commercial_document_holds','commercial_contract_holds','commercial_renewal_holds','commercial_legacy_renewal_holds'] LOOP
  EXECUTE format('ALTER TABLE %I ADD COLUMN incarnation_id uuid NOT NULL DEFAULT gen_random_uuid(), ADD COLUMN review_version bigint NOT NULL DEFAULT 0 CHECK(review_version>=0), ADD COLUMN review_command_id uuid REFERENCES commercial_journal(command_id)',t);
 END LOOP;
END $$;

-- Installation gives old rows identity, not a fabricated historical review.
-- Every later INSERT (even one supplying old metadata) starts a new incarnation.
CREATE FUNCTION commercial_hold_insert() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 NEW.incarnation_id:=gen_random_uuid(); NEW.review_version:=0; NEW.review_command_id:=NULL;
 RETURN NEW;
END $$;

-- Pure validation of an already selected row and its exact journal link: this
-- function performs no second query inside the queue's single data projection.
CREATE FUNCTION commercial_hold_last_review(h jsonb,j jsonb,p_subject uuid) RETURNS jsonb
LANGUAGE plpgsql SET DateStyle='ISO,YMD' SET TimeZone='UTC' AS $$
DECLARE r jsonb; q jsonb; version bigint; previous bigint;
BEGIN
 version:=(h->>'review_version')::bigint;
 IF version=0 AND h->>'review_command_id' IS NULL THEN RETURN NULL; END IF;
 IF version<=0 OR j IS NULL OR j='null'::jsonb THEN RAISE EXCEPTION 'Hold review history unavailable'; END IF;
 r:=j->'result'; q:=j->'request'; previous:=version-1;
 IF j->>'kind' IS DISTINCT FROM 'hold_review' OR j->>'actor' IS NULL
  OR j->>'command_id' IS DISTINCT FROM h->>'review_command_id'
  OR j->>'case_id' IS DISTINCT FROM h->>'case_id'
  OR q-ARRAY['version','command_id','case_id','subject','hold','expected','decision','review_scope','assessment','next_review_at']<>'{}'::jsonb
  OR q->'version' IS DISTINCT FROM '1'::jsonb OR q->>'command_id' IS DISTINCT FROM j->>'command_id'
  OR q->>'case_id' IS DISTINCT FROM h->>'case_id' OR q->>'subject' IS DISTINCT FROM p_subject::text
  OR q->'hold' IS DISTINCT FROM h->'hold'
  OR q->'expected' IS DISTINCT FROM jsonb_build_object('incarnation_id',h->>'incarnation_id','review_version',previous::text)
  OR q->>'decision' IS DISTINCT FROM 'keep' OR q->>'review_scope' IS DISTINCT FROM 'entire_existing_hold'
  OR jsonb_typeof(q->'assessment') IS DISTINCT FROM 'string' OR length(trim(q->>'assessment'))<20
  OR jsonb_typeof(q->'next_review_at') IS DISTINCT FROM 'string'
  OR (q->>'next_review_at')::timestamptz IS DISTINCT FROM (h->>'review_due_at')::timestamptz
  OR r IS DISTINCT FROM jsonb_build_object('version',1,'command_id',j->>'command_id','case_id',h->>'case_id',
   'subject',p_subject,'hold',h->'hold','incarnation_id',h->>'incarnation_id',
   'previous_review_version',previous::text,'review_version',version::text,
   'previous_review_due_at',r->>'previous_review_due_at','next_review_at',h->>'review_due_at',
   'recorded_at',j->>'recorded_at','status','review_recorded','hold_kept',true,'record_deleted',false,'claims_satisfied',false)
  OR jsonb_typeof(r->'previous_review_due_at') IS DISTINCT FROM 'string'
  OR ((r->>'previous_review_due_at')::timestamptz)::text IS DISTINCT FROM r->>'previous_review_due_at' THEN
  RAISE EXCEPTION 'Hold review history unavailable';
 END IF;
 RETURN jsonb_build_object('command_id',j->>'command_id','actor',j->>'actor','recorded_at',j->>'recorded_at',
  'assessment',q->>'assessment','review_scope',q->>'review_scope','previous_review_version',previous::text,
  'previous_review_due_at',r->>'previous_review_due_at','review_version',version::text,'next_review_at',h->>'review_due_at');
EXCEPTION WHEN invalid_text_representation OR invalid_datetime_format OR datetime_field_overflow OR numeric_value_out_of_range OR invalid_parameter_value THEN
 RAISE EXCEPTION 'Hold review history unavailable';
END $$;

CREATE FUNCTION commercial_hold_update() RETURNS trigger LANGUAGE plpgsql
SET DateStyle='ISO,YMD' SET TimeZone='UTC' AS $$
DECLARE h jsonb; j jsonb; owner_id uuid; kind text; record_key text;
BEGIN
 IF to_jsonb(NEW)-ARRAY['review_due_at','review_version','review_command_id'] IS DISTINCT FROM
    to_jsonb(OLD)-ARRAY['review_due_at','review_version','review_command_id']
  OR OLD.review_version=9223372036854775807 OR NEW.review_version<>OLD.review_version+1
  OR NEW.review_command_id IS NULL OR NEW.review_command_id IS NOT DISTINCT FROM OLD.review_command_id THEN
  RAISE EXCEPTION 'Original hold basis and incarnation are immutable; exact appended review required';
 END IF;
 kind:=TG_ARGV[0]; record_key:=to_jsonb(NEW)->>TG_ARGV[1];
 SELECT subject INTO owner_id FROM commercial_cases WHERE id=NEW.case_id;
 SELECT to_jsonb(x)||jsonb_build_object('recorded_at',x.recorded_at::text) INTO j FROM commercial_journal x WHERE command_id=NEW.review_command_id;
 h:=to_jsonb(NEW)||jsonb_build_object('hold',jsonb_build_object('kind',kind,'record_id',record_key),'review_due_at',NEW.review_due_at::text);
 PERFORM commercial_hold_last_review(h,j,owner_id);
 IF j->'result'->>'previous_review_due_at' IS DISTINCT FROM OLD.review_due_at::text THEN
  RAISE EXCEPTION 'Review must record the exact previous hold date';
 END IF;
 RETURN NEW;
END $$;
DO $$ DECLARE t text; key text; kind text; i integer; BEGIN
 FOR i IN 1..4 LOOP
  t:=(ARRAY['commercial_document_holds','commercial_contract_holds','commercial_renewal_holds','commercial_legacy_renewal_holds'])[i];
  key:=(ARRAY['number','declaration_id','agreement_id','user_id'])[i];
  kind:=(ARRAY['financial_document','contract_declaration','renewal_agreement','legacy_renewal'])[i];
  EXECUTE format('CREATE TRIGGER commercial_hold_insert BEFORE INSERT ON %I FOR EACH ROW EXECUTE FUNCTION commercial_hold_insert()',t);
  EXECUTE format('CREATE TRIGGER commercial_hold_update BEFORE UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION commercial_hold_update(%L,%L)',t,kind,key);
 END LOOP;
END $$;

CREATE FUNCTION commercial_hold_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb
LANGUAGE plpgsql SET DateStyle='ISO,YMD' SET TimeZone='UTC' AS $$
DECLARE b jsonb:=p_body-ARRAY['_staff_session','_staff_refresh_hash']; old_command commercial_journal;
 command uuid; c uuid; owner_id uuid; kind text; key text; inc uuid; expected bigint; h jsonb; j jsonb;
 table_name text; key_name text; result jsonb; recorded timestamptz; next_date timestamptz;
 cur jsonb; cursor_date timestamptz; cursor_rank integer; cursor_case uuid; cursor_key text; cursor_inc uuid;
 page_limit integer; all_rows jsonb; last_row jsonb; rows jsonb; more boolean;
BEGIN
 IF current_setting('transaction_isolation')<>'read committed' THEN RAISE EXCEPTION 'Hold operations require READ COMMITTED, including replay and queue'; END IF;
 IF jsonb_typeof(p_body) IS DISTINCT FROM 'object'
  OR jsonb_typeof(p_body->'_staff_session') IS DISTINCT FROM 'string'
  OR jsonb_typeof(p_body->'_staff_refresh_hash') IS DISTINCT FROM 'string'
  OR b->'version' IS DISTINCT FROM '1'::jsonb THEN RAISE EXCEPTION 'Exact versioned hold operation required'; END IF;
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Current administrator MFA authority required'; END IF;
 IF p_operation='hold_queue' THEN
  IF NOT (b ?& ARRAY['version','limit','cursor']) OR b-ARRAY['version','limit','cursor']<>'{}'::jsonb
   OR jsonb_typeof(b->'limit') IS DISTINCT FROM 'number' OR b->>'limit' !~ '^[0-9]+$' THEN RAISE EXCEPTION 'Exact hold queue fields required'; END IF;
  page_limit:=(b->>'limit')::integer;
  IF page_limit<1 OR page_limit>100 THEN RAISE EXCEPTION 'Hold queue limit must be 1 through 100'; END IF;
  cur:=b->'cursor';
  IF cur<>'null'::jsonb THEN
   IF jsonb_typeof(cur) IS DISTINCT FROM 'object' OR NOT(cur ?& ARRAY['review_due_at','kind','case_id','record_id','incarnation_id'])
    OR cur-ARRAY['review_due_at','kind','case_id','record_id','incarnation_id']<>'{}'::jsonb
    OR EXISTS(SELECT 1 FROM jsonb_each(cur) e WHERE jsonb_typeof(e.value)<>'string') THEN RAISE EXCEPTION 'Exact hold cursor required'; END IF;
   cursor_date:=(cur->>'review_due_at')::timestamptz;
   IF cursor_date::text<>cur->>'review_due_at' THEN RAISE EXCEPTION 'Canonical native timestamp cursor required'; END IF;
   cursor_rank:=array_position(ARRAY['financial_document','contract_declaration','renewal_agreement','legacy_renewal'],cur->>'kind');
   cursor_case:=(cur->>'case_id')::uuid; cursor_inc:=(cur->>'incarnation_id')::uuid; cursor_key:=cur->>'record_id';
   IF cursor_rank IS NULL OR cursor_key='' OR cursor_case::text<>cur->>'case_id' OR cursor_inc::text<>cur->>'incarnation_id'
    OR (cursor_rank<>1 AND (cursor_key::uuid)::text<>cursor_key) THEN RAISE EXCEPTION 'Canonical hold cursor identity required'; END IF;
  END IF;
  WITH holds AS (
   SELECT 1 AS rank,'financial_document'::text AS kind,queued_hold.number AS record_key,queued_hold.case_id,queued_hold.incarnation_id,queued_hold.review_version,queued_hold.review_command_id,queued_hold.review_due_at,queued_hold.basis FROM commercial_document_holds queued_hold
   UNION ALL SELECT 2,'contract_declaration',queued_hold.declaration_id::text,queued_hold.case_id,queued_hold.incarnation_id,queued_hold.review_version,queued_hold.review_command_id,queued_hold.review_due_at,queued_hold.basis FROM commercial_contract_holds queued_hold
   UNION ALL SELECT 3,'renewal_agreement',queued_hold.agreement_id::text,queued_hold.case_id,queued_hold.incarnation_id,queued_hold.review_version,queued_hold.review_command_id,queued_hold.review_due_at,queued_hold.basis FROM commercial_renewal_holds queued_hold
   UNION ALL SELECT 4,'legacy_renewal',queued_hold.user_id::text,queued_hold.case_id,queued_hold.incarnation_id,queued_hold.review_version,queued_hold.review_command_id,queued_hold.review_due_at,queued_hold.basis FROM commercial_legacy_renewal_holds queued_hold
  ), page AS MATERIALIZED (
   SELECT queued_hold.*,x.subject FROM holds queued_hold JOIN commercial_cases x ON x.id=queued_hold.case_id
   WHERE cur='null'::jsonb OR (queued_hold.review_due_at,queued_hold.rank,queued_hold.case_id,queued_hold.record_key COLLATE "C",queued_hold.incarnation_id)>
    (cursor_date,cursor_rank,cursor_case,cursor_key COLLATE "C",cursor_inc)
   ORDER BY queued_hold.review_due_at,queued_hold.rank,queued_hold.case_id,queued_hold.record_key COLLATE "C",queued_hold.incarnation_id LIMIT page_limit+1
  ) SELECT coalesce(jsonb_agg(jsonb_build_object('case_id',queued_hold.case_id,'subject',queued_hold.subject,
    'hold',jsonb_build_object('kind',queued_hold.kind,'record_id',queued_hold.record_key),'incarnation_id',queued_hold.incarnation_id,
    'review_version',queued_hold.review_version::text,'review_due_at',queued_hold.review_due_at::text,'basis',queued_hold.basis,
    'last_review',commercial_hold_last_review(to_jsonb(queued_hold)||jsonb_build_object('hold',jsonb_build_object('kind',queued_hold.kind,'record_id',queued_hold.record_key),'review_due_at',queued_hold.review_due_at::text),
     CASE WHEN queued_journal.command_id IS NULL THEN NULL ELSE to_jsonb(queued_journal)||jsonb_build_object('recorded_at',queued_journal.recorded_at::text) END,queued_hold.subject))
    ORDER BY queued_hold.review_due_at,queued_hold.rank,queued_hold.case_id,queued_hold.record_key COLLATE "C",queued_hold.incarnation_id),'[]'::jsonb)
   INTO all_rows FROM page queued_hold LEFT JOIN commercial_journal queued_journal ON queued_journal.command_id=queued_hold.review_command_id;
  IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Administrator authority changed while reading hold queue'; END IF;
  more:=jsonb_array_length(all_rows)>page_limit;
  SELECT coalesce(jsonb_agg(value ORDER BY ord),'[]'::jsonb) INTO rows FROM jsonb_array_elements(all_rows) WITH ORDINALITY a(value,ord) WHERE ord<=page_limit;
  last_row:=rows->(jsonb_array_length(rows)-1);
  RETURN jsonb_build_object('version',1,'queue','four_existing_hold_families','mode','live','observed_at',clock_timestamp()::text,'rows',rows,
   'next_cursor',CASE WHEN more THEN jsonb_build_object('review_due_at',last_row->>'review_due_at','kind',last_row->'hold'->>'kind',
    'case_id',last_row->>'case_id','record_id',last_row->'hold'->>'record_id','incarnation_id',last_row->>'incarnation_id') ELSE NULL END,'exhausted',NOT more);
 END IF;
 IF p_operation<>'hold_review' OR NOT(b ?& ARRAY['version','command_id','case_id','subject','hold','expected','decision','review_scope','assessment','next_review_at'])
  OR b-ARRAY['version','command_id','case_id','subject','hold','expected','decision','review_scope','assessment','next_review_at']<>'{}'::jsonb
  OR EXISTS(SELECT 1 FROM jsonb_each(b-ARRAY['version','hold','expected']) e WHERE jsonb_typeof(e.value)<>'string')
  OR jsonb_typeof(b->'hold') IS DISTINCT FROM 'object' OR NOT(b->'hold' ?& ARRAY['kind','record_id']) OR (b->'hold')-ARRAY['kind','record_id']<>'{}'::jsonb
  OR EXISTS(SELECT 1 FROM jsonb_each(b->'hold') e WHERE jsonb_typeof(e.value)<>'string')
  OR jsonb_typeof(b->'expected') IS DISTINCT FROM 'object' OR NOT(b->'expected' ?& ARRAY['incarnation_id','review_version']) OR (b->'expected')-ARRAY['incarnation_id','review_version']<>'{}'::jsonb
  OR EXISTS(SELECT 1 FROM jsonb_each(b->'expected') e WHERE jsonb_typeof(e.value)<>'string')
  OR b->>'decision'<>'keep' OR b->>'review_scope'<>'entire_existing_hold' OR length(trim(b->>'assessment'))<20
  OR b->'expected'->>'review_version' !~ '^(0|[1-9][0-9]*)$' THEN RAISE EXCEPTION 'Exact existing-hold keep review required'; END IF;
 command:=(b->>'command_id')::uuid; c:=(b->>'case_id')::uuid; owner_id:=(b->>'subject')::uuid;
 inc:=(b->'expected'->>'incarnation_id')::uuid; expected:=(b->'expected'->>'review_version')::bigint;
 kind:=b->'hold'->>'kind'; key:=b->'hold'->>'record_id';
 cursor_rank:=array_position(ARRAY['financial_document','contract_declaration','renewal_agreement','legacy_renewal'],kind);
 IF cursor_rank IS NULL OR key='' OR command::text<>b->>'command_id' OR c::text<>b->>'case_id' OR owner_id::text<>b->>'subject'
  OR inc::text<>b->'expected'->>'incarnation_id' OR (cursor_rank<>1 AND (key::uuid)::text<>key) THEN RAISE EXCEPTION 'Canonical exact hold identity required'; END IF;
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Administrator authority changed while waiting for command'; END IF;
 SELECT * INTO old_command FROM commercial_journal WHERE command_id=command;
 IF FOUND THEN
  IF old_command.actor IS DISTINCT FROM p_actor OR old_command.kind<>'hold_review' OR old_command.request<>b THEN RAISE EXCEPTION 'Conflicting hold command replay'; END IF;
  RETURN old_command.result;
 END IF;
 PERFORM 1 FROM users WHERE id=owner_id FOR UPDATE;
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Administrator authority changed while waiting for original user'; END IF;
 PERFORM 1 FROM commercial_cases WHERE id=c AND subject=owner_id FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'Exact existing original case required'; END IF;
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Administrator authority changed while waiting for case'; END IF;
 table_name:=(ARRAY['commercial_document_holds','commercial_contract_holds','commercial_renewal_holds','commercial_legacy_renewal_holds'])[cursor_rank];
 key_name:=(ARRAY['number','declaration_id','agreement_id','user_id'])[cursor_rank];
 EXECUTE format('SELECT to_jsonb(h)||jsonb_build_object(''review_due_at'',h.review_due_at::text) FROM %I h WHERE case_id=$1 AND %I=$2%s FOR UPDATE',table_name,key_name,CASE WHEN cursor_rank=1 THEN '' ELSE '::uuid' END) INTO h USING c,key;
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Administrator authority changed while waiting for hold'; END IF;
 IF h IS NULL OR (h->>'incarnation_id')::uuid<>inc OR (h->>'review_version')::bigint<>expected THEN RAISE EXCEPTION 'Hold absent, replaced or already reviewed'; END IF;
 SELECT to_jsonb(x)||jsonb_build_object('recorded_at',x.recorded_at::text) INTO j FROM commercial_journal x WHERE command_id=(h->>'review_command_id')::uuid;
 PERFORM commercial_hold_last_review(h||jsonb_build_object('hold',b->'hold'),j,owner_id);
 recorded:=clock_timestamp();
 IF b->>'next_review_at' !~ '^[0-9]{4,6}-[0-9]{2}-[0-9]{2}[T ][0-9]{2}:[0-9]{2}(:[0-9]{2}([.][0-9]{1,6})?)?(Z|[+-][0-9]{2}(:?[0-9]{2})?)$' THEN RAISE EXCEPTION 'Finite future review date with explicit timezone required'; END IF;
 next_date:=(b->>'next_review_at')::timestamptz;
 IF NOT isfinite(next_date) OR next_date<=recorded OR expected=9223372036854775807 THEN RAISE EXCEPTION 'Finite future date and available review revision required'; END IF;
 result:=jsonb_build_object('version',1,'command_id',command,'case_id',c,'subject',owner_id,'hold',b->'hold','incarnation_id',inc,
  'previous_review_version',expected::text,'review_version',(expected+1)::text,'previous_review_due_at',h->>'review_due_at',
  'next_review_at',next_date::text,'recorded_at',recorded::text,'status','review_recorded','hold_kept',true,'record_deleted',false,'claims_satisfied',false);
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Administrator authority changed before review write'; END IF;
 INSERT INTO commercial_journal(case_id,actor,command_id,kind,request,result,recorded_at) VALUES(c,p_actor,command,'hold_review',b,result,recorded);
 EXECUTE format('UPDATE %I SET review_due_at=$3,review_version=review_version+1,review_command_id=$4 WHERE case_id=$1 AND %I=$2%s',table_name,key_name,CASE WHEN cursor_rank=1 THEN '' ELSE '::uuid' END) USING c,key,next_date,command;
 RETURN result;
EXCEPTION WHEN invalid_text_representation OR invalid_datetime_format OR datetime_field_overflow OR numeric_value_out_of_range OR invalid_parameter_value THEN
 RAISE EXCEPTION 'Malformed or out-of-range hold identity, revision or timestamp';
END $$;

-- Preserve every operation of the immediate accepted staff/IF1 predecessor.
ALTER FUNCTION commercial_operation(text,uuid,jsonb) RENAME TO commercial_operation_before_hold_review;
CREATE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation IN ('hold_review','hold_queue') THEN RETURN commercial_hold_operation(p_operation,p_actor,p_body); END IF;
 RETURN commercial_operation_before_hold_review(p_operation,p_actor,p_body);
END $$;
