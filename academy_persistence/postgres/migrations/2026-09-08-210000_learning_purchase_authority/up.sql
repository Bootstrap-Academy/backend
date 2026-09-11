-- New spending requires a current claimant election for the exact original
-- offer. A short-lived learning key only authorizes already-granted use.
ALTER TABLE commercial_purchase_authorizations ADD COLUMN claim_epoch bigint NOT NULL;
ALTER TABLE commercial_purchase_authorizations ADD COLUMN contact_epoch bigint NOT NULL;
CREATE FUNCTION commercial_purchase_lock(p_subject uuid) RETURNS boolean LANGUAGE plpgsql AS $$
DECLARE c uuid;
BEGIN
 PERFORM 1 FROM users WHERE id=p_subject FOR UPDATE;
 IF NOT FOUND THEN RETURN false; END IF;
 SELECT case_id INTO c FROM commercial_learning_subjects WHERE subject=p_subject;
 IF c IS NOT NULL THEN
  PERFORM 1 FROM commercial_cases WHERE id=c FOR UPDATE;
  -- The live service row precedes the claimant and order locks. This does not
  -- change T6's exact-receipt-first or T12's pre-lock receipt observations.
  IF NOT commercial_learning_allowed(p_subject) THEN RAISE EXCEPTION 'Current limited-service admission unavailable; existing rights preserved'; END IF;
 END IF;
 RETURN true;
END $$;
CREATE FUNCTION commercial_purchase_authorize(p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c commercial_cases; service_id uuid; o purchase_offers; a commercial_purchase_authorizations;
 command uuid:=(p_body->>'command_id')::uuid; payload jsonb:=p_body-'_claim_hash'-'_moderation_hash'; receipt commercial_journal; result jsonb;
BEGIN
 SELECT * INTO c FROM commercial_cases WHERE subject=p_actor;
 IF c.id IS NULL OR NOT commercial_personal_proof(c.id,p_body) THEN RAISE EXCEPTION 'Current personal claimant proof required'; END IF;
 IF command IS NULL THEN RAISE EXCEPTION 'Exact request identity required'; END IF;
 -- Same ordering as commercial_claim_operation: serialize this command before
 -- its user/case prerequisites. An identical waiter reads the committed receipt
 -- before consulting offer state changed by the successful first request.
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF NOT commercial_personal_proof(c.id,p_body) THEN RAISE EXCEPTION 'Claimant proof changed while waiting for original command'; END IF;
 SELECT * INTO receipt FROM commercial_journal WHERE command_id=command;
 IF FOUND THEN
  IF receipt.case_id<>c.id OR receipt.actor<>p_actor OR receipt.kind<>'purchase_authorize' OR receipt.request<>payload THEN RAISE EXCEPTION 'Conflicting purchase authorization command'; END IF;
  RETURN receipt.result;
 END IF;
 SELECT l.subject INTO service_id FROM purchase_offers x JOIN commercial_learning_subjects l ON l.subject=x.user_id
 WHERE x.id=(p_body->'acceptance'->>'order_id')::uuid AND l.case_id=c.id AND l.erased_at IS NULL;
 IF service_id IS NULL OR NOT commercial_purchase_lock(service_id) THEN RAISE EXCEPTION 'Current owned service subject required'; END IF;
 SELECT * INTO c FROM commercial_cases WHERE id=c.id;
 IF NOT commercial_personal_proof(c.id,p_body) THEN RAISE EXCEPTION 'Claimant proof changed while waiting'; END IF;
 SELECT x.* INTO o FROM purchase_offers x JOIN purchase_progress p ON p.order_id=x.id
 WHERE x.id=(p_body->'acceptance'->>'order_id')::uuid AND p.state='offered' FOR UPDATE OF p;
 IF o.id IS NULL OR o.expires_at<=clock_timestamp() OR o.source NOT IN ('backend','skills','events')
  OR o.offer->>'hash' IS DISTINCT FROM p_body->'acceptance'->>'offer_hash'
  OR p_body->'acceptance'->'accepted' IS DISTINCT FROM 'true'::jsonb
  OR ((o.offer->'product'->>'coins')::bigint>0 AND p_body->'acceptance'->'early_performance_requested' IS DISTINCT FROM 'true'::jsonb)
  OR p_body->'acceptance' IS DISTINCT FROM jsonb_build_object('order_id',o.id,'offer_hash',o.offer->>'hash','accepted',true,'early_performance_requested',p_body->'acceptance'->'early_performance_requested') THEN
  RAISE EXCEPTION 'Exact current original offer and explicit acceptance required';
 END IF;
 SELECT * INTO a FROM commercial_purchase_authorizations WHERE order_id=o.id;
 IF FOUND THEN RAISE EXCEPTION 'Preserve and retry the original authorization command'; END IF;
 INSERT INTO commercial_purchase_authorizations(order_id,subject,case_id,command_id,expires_at,original_offer,exact_acceptance,proof,claim_epoch,contact_epoch)
 VALUES(o.id,service_id,c.id,command,least(o.expires_at,clock_timestamp()+interval '15 minutes'),o.offer,p_body->'acceptance',
  jsonb_build_object('actor',p_actor,'scope','exact new offer acceptance','proof_kind',CASE WHEN p_body ? '_claim_hash' THEN 'current_claim_key' ELSE 'current_personal_rights_proof' END),c.access_epoch,c.contact_epoch);
 result:=jsonb_build_object('order_id',o.id,'subject',service_id,'authorized',true,'accepted',false,'charged',false,
  'expires_at',(SELECT expires_at FROM commercial_purchase_authorizations WHERE order_id=o.id));
 INSERT INTO commercial_journal(case_id,actor,command_id,kind,request,result) VALUES(c.id,p_actor,command,'purchase_authorize',payload,result);
 RETURN result;
END $$;
CREATE FUNCTION commercial_purchase_submission_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE o purchase_offers; a commercial_purchase_authorizations; c commercial_cases;
BEGIN
 SELECT * INTO o FROM purchase_offers WHERE id=NEW.order_id;
 IF NOT commercial_is_learning_subject(o.user_id) THEN RETURN NEW; END IF;
 IF NOT commercial_purchase_lock(o.user_id) THEN RAISE EXCEPTION 'Erased service subject cannot accept a new contract'; END IF;
 SELECT * INTO a FROM commercial_purchase_authorizations WHERE order_id=o.id FOR UPDATE;
 SELECT * INTO c FROM commercial_cases WHERE id=a.case_id;
 IF a.order_id IS NULL OR a.revoked_at IS NOT NULL OR a.consumed_at IS NOT NULL OR a.expires_at<=clock_timestamp()
  OR a.original_offer<>o.offer OR a.exact_acceptance<>NEW.request OR a.claim_epoch<>c.access_epoch OR a.contact_epoch<>c.contact_epoch THEN
  RAISE EXCEPTION 'Separate current claimant authorization for this exact offer is required';
 END IF;
 UPDATE commercial_purchase_authorizations SET consumed_at=clock_timestamp() WHERE order_id=o.id;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_purchase_submission_guard BEFORE INSERT ON purchase_submissions FOR EACH ROW EXECUTE FUNCTION commercial_purchase_submission_guard();
CREATE FUNCTION commercial_purchase_acceptance_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM purchase_offers o WHERE o.id=NEW.order_id AND commercial_is_learning_subject(o.user_id))
  AND NOT EXISTS(SELECT 1 FROM commercial_purchase_authorizations a JOIN purchase_submissions s ON s.order_id=a.order_id
   WHERE a.order_id=NEW.order_id AND a.consumed_at IS NOT NULL AND a.revoked_at IS NULL AND a.exact_acceptance=s.request) THEN
  RAISE EXCEPTION 'Limited-service acceptance requires the original separately authorized submission';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_purchase_acceptance_guard BEFORE INSERT ON purchase_acceptances FOR EACH ROW EXECUTE FUNCTION commercial_purchase_acceptance_guard();
CREATE FUNCTION commercial_purchase_authorization_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF to_jsonb(NEW)-'consumed_at'-'revoked_at'<>to_jsonb(OLD)-'consumed_at'-'revoked_at'
  OR (OLD.consumed_at IS NOT NULL AND NEW.consumed_at IS DISTINCT FROM OLD.consumed_at)
  OR (OLD.revoked_at IS NOT NULL AND NEW.revoked_at IS DISTINCT FROM OLD.revoked_at) THEN RAISE EXCEPTION 'Original claimant authorization is immutable'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_purchase_authorization_immutable BEFORE UPDATE ON commercial_purchase_authorizations FOR EACH ROW EXECUTE FUNCTION commercial_purchase_authorization_immutable();
CREATE OR REPLACE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation='purchase_authorize' THEN RETURN commercial_purchase_authorize(p_actor,p_body); END IF;
 IF p_operation IN ('learning_start','learning_access','learning_summary','learning_revoke','learning_authority') THEN
  RETURN commercial_learning_operation(p_operation,p_actor,p_body);
 END IF;
 RETURN commercial_claim_operation(p_operation,p_actor,p_body);
END $$;


CREATE OR REPLACE FUNCTION commercial_learning_allowed(p_subject uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM commercial_learning_subjects l JOIN commercial_cases c ON c.id=l.case_id JOIN users u ON u.id=l.subject
 WHERE l.subject=p_subject AND l.erased_at IS NULL AND c.contact_verified AND c.contact IS NOT NULL
 AND NOT EXISTS(SELECT 1 FROM moderation_holds h WHERE h.target_kind='account' AND h.target_id IN(c.subject,l.subject)
  AND h.active AND h.effect IN ('restrict','hide','remove') AND h.starts_at<=clock_timestamp() AND (h.ends_at IS NULL OR h.ends_at>clock_timestamp()))
 AND NOT EXISTS(SELECT 1 FROM commercial_learning_restrictions r WHERE r.case_id=c.id AND r.resolved_at IS NULL));
$$;

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
 SELECT subject INTO subject_id FROM commercial_learning_subjects WHERE case_id=c AND erased_at IS NULL;
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
