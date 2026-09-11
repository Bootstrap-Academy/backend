-- Same-subject refresh only; original learning receipts and all other branches remain unchanged.
CREATE OR REPLACE FUNCTION commercial_learning_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c uuid; subject_id uuid; command uuid; receipt commercial_journal; payload jsonb; result jsonb;
BEGIN
 IF p_operation='learning_authority' THEN
  SELECT l.subject INTO subject_id FROM commercial_learning_keys k JOIN commercial_learning_subjects l ON l.subject=k.subject JOIN commercial_cases c ON c.id=l.case_id
  WHERE k.hash=p_body->>'hash' AND k.revoked_at IS NULL AND k.expires_at>clock_timestamp() AND k.epoch=l.authority_epoch AND k.claim_epoch=c.access_epoch AND k.contact_epoch=c.contact_epoch AND l.erased_at IS NULL AND commercial_learning_allowed(l.subject);
  IF subject_id IS NULL THEN RETURN NULL; END IF;
  RETURN jsonb_build_object('subject',subject_id,'purpose','retained_learning','ordinary_authority',false,'financial_authority',false,'email_verified',true,'admin',false);
 END IF;
 IF p_operation NOT IN ('learning_start','learning_access','learning_summary','learning_revoke') THEN RAISE EXCEPTION 'Unsupported learning operation'; END IF;
 c:=commercial_lock_subject(p_actor);
 IF NOT commercial_personal_proof(c,p_body) THEN RAISE EXCEPTION 'Current personal claimant proof required'; END IF;
 IF p_operation='learning_summary' THEN
  RETURN jsonb_build_object('subjects',coalesce((SELECT jsonb_agg(to_jsonb(l)) FROM commercial_learning_subjects l WHERE l.case_id=c),'[]'),
   'scope','Separate technical service subjects; existing claims and original contracts remain authoritative');
 END IF;
 command:=(p_body->>'command_id')::uuid;
 IF command IS NULL THEN RAISE EXCEPTION 'Exact request identity required'; END IF;
 -- The claimant lock already serializes this family's operations. The command
 -- must not acquire a second user's/case's lock on a conflicting replay.
 SELECT * INTO receipt FROM commercial_journal WHERE command_id=command;
 payload:=p_body-'_claim_hash'-'_moderation_hash'-'hash';
 IF FOUND THEN
  IF receipt.case_id<>c OR receipt.kind<>p_operation OR receipt.actor<>p_actor OR receipt.request<>payload THEN RAISE EXCEPTION 'Conflicting learning command'; END IF;
  IF p_operation IN ('learning_start','learning_access') AND NOT EXISTS(SELECT 1 FROM commercial_learning_keys WHERE command_id=command AND hash=p_body->>'hash') THEN RAISE EXCEPTION 'Conflicting learning credential replay'; END IF;
  RETURN receipt.result;
 END IF;
 -- An exact saved receipt above predates this requirement and remains replayable.
 -- A fresh refresh may only reissue for the selected existing subject, even if
 -- another subject was legitimately elected after the caller read its summary.
 IF p_operation='learning_access' THEN
  IF jsonb_typeof(p_body->'expected_subject') IS DISTINCT FROM 'string' THEN
   RAISE EXCEPTION 'Exact existing learning subject required';
  END IF;
  SELECT subject INTO subject_id FROM commercial_learning_subjects
   WHERE case_id=c AND subject=(p_body->>'expected_subject')::uuid AND erased_at IS NULL;
 ELSE
  SELECT subject INTO subject_id FROM commercial_learning_subjects WHERE case_id=c AND erased_at IS NULL;
 END IF;
 IF p_operation='learning_start' AND subject_id IS NULL THEN
  IF p_body->'use_retained_value' IS DISTINCT FROM 'true'::jsonb THEN RAISE EXCEPTION 'Explicit choice of limited learning space required'; END IF;
  IF NOT EXISTS(SELECT 1 FROM commercial_cases WHERE id=c AND contact_verified AND contact IS NOT NULL) THEN RAISE EXCEPTION 'Verified current commercial contact required'; END IF;
  subject_id:=gen_random_uuid();
  INSERT INTO commercial_learning_subjects(subject,case_id,election_id,election)
   VALUES(subject_id,c,command,payload||jsonb_build_object('actor',p_actor,'decision','fresh limited learning subject; no general contract/age/representative acceptance inferred'));
  PERFORM set_config('academy.learning_identity_write',subject_id::text,true);
  INSERT INTO users(id,name,email,email_verified,created_at,enabled,admin)
   VALUES(subject_id,'claim_'||substr(replace(subject_id::text,'-',''),1,26),NULL,false,clock_timestamp(),false,false);
  INSERT INTO user_profiles(user_id,display_name,bio,tags,leaderboard_opt_out) VALUES(subject_id,'Lernzugang','',ARRAY[]::text[],true);
  INSERT INTO user_invoice_info(user_id) VALUES(subject_id);
  INSERT INTO moderation_targets(kind,id,subject,base_enabled) VALUES('account',subject_id,subject_id,true);
  PERFORM set_config('academy.learning_identity_write','',true);
 END IF;
 IF subject_id IS NULL THEN RAISE EXCEPTION 'An explicitly elected service subject is required'; END IF;
 IF p_operation='learning_revoke' THEN
  UPDATE commercial_learning_subjects SET authority_epoch=authority_epoch+1 WHERE subject=subject_id;
  UPDATE commercial_learning_keys SET revoked_at=clock_timestamp() WHERE subject=subject_id AND revoked_at IS NULL;
  result:=jsonb_build_object('revoked',true,'claims_satisfied',false);
 ELSE
  IF p_body->>'hash' IS NULL OR p_body->>'hash' !~ '^[0-9a-f]{64}$' THEN RAISE EXCEPTION 'Pre-retained random learning credential required'; END IF;
  IF NOT commercial_learning_allowed(subject_id) THEN RAISE EXCEPTION 'Current scoped learning admission unavailable; existing claims preserved'; END IF;
  INSERT INTO commercial_learning_keys(hash,subject,epoch,claim_epoch,contact_epoch,command_id,expires_at)
  SELECT p_body->>'hash',subject_id,l.authority_epoch,x.access_epoch,x.contact_epoch,command,clock_timestamp()+interval '15 minutes' FROM commercial_learning_subjects l JOIN commercial_cases x ON x.id=l.case_id WHERE l.subject=subject_id;
  result:=jsonb_build_object('subject',subject_id,'expires_at',(SELECT expires_at FROM commercial_learning_keys WHERE hash=p_body->>'hash'),
    'purpose','retained_learning','ordinary_authority',false,'financial_authority',false,'claims_satisfied',false);
 END IF;
 INSERT INTO commercial_journal(case_id,actor,command_id,kind,request,result) VALUES(c,p_actor,command,p_operation,payload,result);
 RETURN result;
END $$;
