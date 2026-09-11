-- Conservative necessary-evidence preservation. No identity resolution, new
-- disposal decision or change to original-reader ownership/content admission.
-- Historical dispatchers and migrations remain byte-identical.
CREATE FUNCTION commercial_invoice_before_delete() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.kind<>'invoice' THEN RETURN OLD; END IF;
 IF current_setting('transaction_isolation')<>'read committed' THEN
  RAISE EXCEPTION USING ERRCODE='25001', MESSAGE='Invoice deletion requires READ COMMITTED';
 END IF;
 -- Repository pruning has acquired archive before document. Do not acquire an
 -- archive lock underneath this row (including direct SQL DELETE callers).
 PERFORM commercial_capture_retention_owner(OLD.number,OLD.kind,OLD.user_id);
 IF commercial_invoice_identity_pending(OLD.number) THEN RETURN NULL; END IF;
 RETURN OLD;
END $$;
CREATE TRIGGER commercial_invoice_before_delete BEFORE DELETE ON financial_documents
 FOR EACH ROW EXECUTE FUNCTION commercial_invoice_before_delete();

-- Check current staff after command wait even for an exact old replay. New
-- archive review locks command -> archive -> work; disposal never asks for command.
CREATE OR REPLACE FUNCTION commercial_retention_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE command uuid; old_command commercial_journal; request_body jsonb; result jsonb;
BEGIN
 IF current_setting('transaction_isolation')<>'read committed' THEN RAISE EXCEPTION USING ERRCODE='25001', MESSAGE='Retention review requires READ COMMITTED'; END IF;
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Current administrator MFA authority required'; END IF;
 command:=(p_body->>'command_id')::uuid;
 IF command IS NULL OR length(coalesce(p_body->>'assessment',''))<20 OR nullif(p_body->>'next_review_at','')::timestamptz IS NULL THEN RAISE EXCEPTION 'Exact review identity, necessity assessment and next review required'; END IF;
 request_body:=p_body-'_staff_session'-'_staff_refresh_hash'-'_claim_hash'-'_moderation_hash';
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Staff authority changed while waiting'; END IF;
 SELECT * INTO old_command FROM commercial_journal WHERE command_id=command;
 IF FOUND THEN
  IF old_command.actor IS DISTINCT FROM p_actor OR old_command.kind<>p_operation OR old_command.request<>request_body THEN RAISE EXCEPTION 'Conflicting retention replay'; END IF;
  RETURN old_command.result;
 END IF;
 IF p_body->'authorize_disposal' IS DISTINCT FROM 'true'::jsonb AND p_body->'authorize_disposal' IS DISTINCT FROM 'false'::jsonb THEN RAISE EXCEPTION 'Explicit disposal decision required'; END IF;
 IF p_body->'authorize_disposal'='true'::jsonb AND
  (p_body->'remaining_claims_assessed' IS DISTINCT FROM 'true'::jsonb OR p_body->'document_not_necessary' IS DISTINCT FROM 'true'::jsonb
   OR length(coalesce(p_body->>'alternative_evidence',''))<40) THEN RAISE EXCEPTION 'Assess independent claims and necessary replacement evidence; timestamp is not proof of payment'; END IF;
 IF p_operation='statement_review' THEN
  PERFORM 1 FROM financial_documents WHERE number=p_body->>'number' AND kind='final_statement' FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'Existing original statement required'; END IF;
  IF p_body->'authorize_disposal'='true'::jsonb AND EXISTS(SELECT 1 FROM commercial_document_holds WHERE number=p_body->>'number') THEN RAISE EXCEPTION 'Independent claim hold requires its own justified release'; END IF;
  IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Staff authority changed while waiting'; END IF;
  PERFORM commercial_capture_retention_owner(p_body->>'number','final_statement');
  UPDATE commercial_statement_disposal_reviews SET authorized=(p_body->>'authorize_disposal')::boolean,
   review_due_at=(p_body->>'next_review_at')::timestamptz,assessment=request_body||jsonb_build_object('actor',p_actor)
   WHERE number=p_body->>'number';
 ELSE
  PERFORM pg_advisory_xact_lock(hashtextextended('commercial-archive:'||(p_body->>'number'),0));
  IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Staff authority changed while waiting'; END IF;
  PERFORM 1 FROM commercial_archive_work WHERE number=p_body->>'number' AND kind=p_body->>'kind' FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'Pending unrecorded archive required'; END IF;
  IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Staff authority changed while waiting'; END IF;
  IF NOT EXISTS(SELECT 1 FROM commercial_archive_work WHERE number=p_body->>'number' AND kind=p_body->>'kind' AND source='unrecorded_archive' AND disposal_started_at IS NULL AND file_removed_at IS NULL) THEN RAISE EXCEPTION 'Pending unrecorded archive required'; END IF;
  IF EXISTS(SELECT 1 FROM financial_documents WHERE number=p_body->>'number') THEN RAISE EXCEPTION 'Recorded original requires its owning retention assessment'; END IF;
  IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Staff authority changed while waiting'; END IF;
  PERFORM commercial_capture_retention_owner(p_body->>'number',p_body->>'kind');
  UPDATE commercial_archive_work SET disposal_authorized=(p_body->>'authorize_disposal')::boolean,
   review_due_at=(p_body->>'next_review_at')::timestamptz,assessment=request_body||jsonb_build_object('actor',p_actor)
   WHERE number=p_body->>'number' AND kind=p_body->>'kind';
 END IF;
 result:=jsonb_build_object('review_recorded',true,'claims_satisfied',false,'document_deleted',false);
 -- Global unassigned-document review deliberately takes no case lock beneath
 -- the document lock; account erasure takes case then document hold.
 INSERT INTO commercial_journal(case_id,actor,command_id,kind,request,result) VALUES(NULL,p_actor,command,p_operation,request_body,result);
 RETURN result;
END $$;
