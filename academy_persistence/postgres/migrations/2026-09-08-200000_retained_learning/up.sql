-- A purpose-limited fresh service subject is not an ordinary account and is
-- never a recreation of an erased identity. The legal claimant remains in the
-- commercial case; this UUID composes new private product grants only.
CREATE TABLE commercial_learning_subjects (
 subject uuid PRIMARY KEY, case_id uuid NOT NULL REFERENCES commercial_cases(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), erased_at timestamptz,
 authority_epoch bigint NOT NULL DEFAULT 1,
 election_id uuid NOT NULL UNIQUE, election jsonb NOT NULL,
 contract_permissions jsonb NOT NULL DEFAULT '{"capacity":"not_inferred","representative":"not_inferred"}'
);
CREATE UNIQUE INDEX commercial_one_active_learning_subject ON commercial_learning_subjects(case_id) WHERE erased_at IS NULL;
CREATE TABLE commercial_learning_keys (
 hash text PRIMARY KEY CHECK(hash ~ '^[0-9a-f]{64}$'), subject uuid NOT NULL REFERENCES commercial_learning_subjects(subject),
 epoch bigint NOT NULL, claim_epoch bigint NOT NULL, contact_epoch bigint NOT NULL, command_id uuid NOT NULL UNIQUE, expires_at timestamptz NOT NULL, revoked_at timestamptz,
 issued_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
-- A learning key never grants this separate exact new-contract authorization.
CREATE TABLE commercial_purchase_authorizations (
 order_id uuid PRIMARY KEY REFERENCES purchase_offers(id), subject uuid NOT NULL REFERENCES commercial_learning_subjects(subject),
 case_id uuid NOT NULL REFERENCES commercial_cases(id), command_id uuid NOT NULL UNIQUE,
 authorized_at timestamptz NOT NULL DEFAULT clock_timestamp(), expires_at timestamptz NOT NULL,
 original_offer jsonb NOT NULL, exact_acceptance jsonb NOT NULL, proof jsonb NOT NULL,
 consumed_at timestamptz, revoked_at timestamptz
);
CREATE TABLE commercial_successor_grants (
 id uuid PRIMARY KEY, case_id uuid NOT NULL REFERENCES commercial_cases(id),
 source text NOT NULL, original_contract text NOT NULL, successor uuid NOT NULL REFERENCES commercial_learning_subjects(subject),
 command_id uuid NOT NULL UNIQUE, original_scope jsonb NOT NULL, claimant_authorization jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), state text NOT NULL CHECK(state IN ('reserved','uncertain','granted','rejected')),
 result jsonb, UNIQUE(source,original_contract,successor)
);
-- Separate scope requests resolve T0/T1/T2 by the actual service subject. A
-- shared claimant does not backdate the cancellation of a later contract.
CREATE TABLE commercial_subject_erasures (
 subject uuid PRIMARY KEY, case_id uuid NOT NULL REFERENCES commercial_cases(id),
 request_id uuid NOT NULL REFERENCES commercial_requests(id),
 erased_at timestamptz NOT NULL, scope text NOT NULL CHECK(scope IN ('ordinary_account','learning_data'))
);
CREATE TRIGGER immutable_commercial_subject_erasure BEFORE UPDATE ON commercial_subject_erasures
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
INSERT INTO commercial_subject_erasures(subject,case_id,request_id,erased_at,scope)
 SELECT c.subject,c.id,r.id,c.erased_at,'ordinary_account' FROM commercial_cases c JOIN commercial_requests r ON r.case_id=c.id AND r.kind='account_erasure' WHERE c.erased_at IS NOT NULL;

CREATE FUNCTION commercial_is_learning_subject(p_subject uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM commercial_learning_subjects WHERE subject=p_subject);
$$;
CREATE FUNCTION commercial_protect_learning_identity() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE subject_id uuid; value jsonb:=to_jsonb(NEW);
BEGIN
 subject_id:=CASE WHEN TG_TABLE_NAME='users' THEN (to_jsonb(NEW)->>'id')::uuid ELSE (to_jsonb(NEW)->>'user_id')::uuid END;
 IF NOT commercial_is_learning_subject(subject_id) THEN RETURN NEW; END IF;
 IF TG_TABLE_NAME='users' AND TG_OP='UPDATE' AND current_setting('academy.moderation_write',true)='authorized'
  AND value-'enabled'=to_jsonb(OLD)-'enabled' THEN
  NEW.enabled:=false; RETURN NEW;
 END IF;
 IF current_setting('academy.learning_identity_write',true) IS DISTINCT FROM subject_id::text THEN
  RAISE EXCEPTION 'Ordinary account mutation is unavailable for a limited service subject';
 END IF;
 IF TG_TABLE_NAME='users' AND ((value->>'enabled')::boolean OR (value->>'admin')::boolean OR value->>'email' IS NOT NULL OR (value->>'email_verified')::boolean
  OR value->>'terms_version' IS NOT NULL OR value->>'terms_accepted_at' IS NOT NULL OR value->>'age_confirmed_at' IS NOT NULL) THEN
  RAISE EXCEPTION 'Limited service subject cannot acquire ordinary identity or inferred general acceptance';
 END IF;
 IF TG_TABLE_NAME='user_profiles' AND (NOT (value->>'leaderboard_opt_out')::boolean OR value->>'bio'<>'' OR jsonb_array_length(value->'tags')>0) THEN
  RAISE EXCEPTION 'Limited service profile remains private';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_protect_learning_identity BEFORE INSERT OR UPDATE ON users FOR EACH ROW EXECUTE FUNCTION commercial_protect_learning_identity();
CREATE TRIGGER commercial_protect_learning_profile BEFORE INSERT OR UPDATE ON user_profiles FOR EACH ROW EXECUTE FUNCTION commercial_protect_learning_identity();
CREATE TRIGGER commercial_protect_learning_billing BEFORE INSERT OR UPDATE ON user_invoice_info FOR EACH ROW EXECUTE FUNCTION commercial_protect_learning_identity();
CREATE FUNCTION commercial_protect_learning_auth() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE subject_id uuid;
BEGIN
 IF TG_TABLE_NAME='session_refresh_tokens' THEN
  SELECT user_id INTO subject_id FROM sessions WHERE id=(to_jsonb(NEW)->>'session_id')::uuid;
 ELSE subject_id:=(to_jsonb(NEW)->>'user_id')::uuid;
 END IF;
 IF commercial_is_learning_subject(subject_id) THEN RAISE EXCEPTION 'Limited service subject has no ordinary session, password or OAuth authority'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_protect_learning_sessions BEFORE INSERT OR UPDATE ON sessions FOR EACH ROW EXECUTE FUNCTION commercial_protect_learning_auth();
CREATE TRIGGER commercial_protect_learning_refresh BEFORE INSERT OR UPDATE ON session_refresh_tokens FOR EACH ROW EXECUTE FUNCTION commercial_protect_learning_auth();
CREATE TRIGGER commercial_protect_learning_password BEFORE INSERT OR UPDATE ON user_passwords FOR EACH ROW EXECUTE FUNCTION commercial_protect_learning_auth();
CREATE TRIGGER commercial_protect_learning_oauth BEFORE INSERT OR UPDATE ON oauth2_links FOR EACH ROW EXECUTE FUNCTION commercial_protect_learning_auth();

-- Keep the same current moderation projection and column schema for ordinary
-- callers, excluding purpose subjects at the common identity-read boundary.
-- Only explicit typed service-recipient methods may read the separate view.
DO $$ DECLARE definition text; BEGIN
 SELECT pg_get_viewdef('user_composites'::regclass,true) INTO definition;
 EXECUTE 'CREATE VIEW commercial_service_composites AS '||definition;
 EXECUTE 'CREATE OR REPLACE VIEW user_composites AS SELECT c.* FROM commercial_service_composites c WHERE NOT commercial_is_learning_subject(c.id)';
END $$;

CREATE FUNCTION commercial_personal_proof(p_case uuid,p_body jsonb) RETURNS boolean LANGUAGE sql AS $$
 SELECT CASE WHEN p_body ? '_claim_hash' THEN EXISTS(
  SELECT 1 FROM commercial_access_keys k JOIN commercial_cases c ON c.id=k.case_id WHERE c.id=p_case AND k.hash=p_body->>'_claim_hash' AND k.revoked_at IS NULL AND k.epoch=c.access_epoch)
 ELSE EXISTS(SELECT 1 FROM moderation_capabilities k JOIN commercial_cases c ON c.subject=k.subject WHERE c.id=p_case AND k.hash=p_body->>'_moderation_hash' AND k.scope='rights' AND k.revoked_at IS NULL AND k.expires_at>clock_timestamp()) END;
$$;
-- A separate evidenced applicability question is neither an expired historical
-- hold nor an automatic consequence of deleting the old account.
CREATE TABLE commercial_learning_restrictions (
 id uuid PRIMARY KEY, case_id uuid NOT NULL REFERENCES commercial_cases(id),
 source_reference text NOT NULL, evidence jsonb NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 review_due_at timestamptz NOT NULL DEFAULT clock_timestamp(), resolved_at timestamptz, resolution jsonb
);
CREATE FUNCTION commercial_learning_allowed(p_subject uuid) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM commercial_learning_subjects l JOIN commercial_cases c ON c.id=l.case_id JOIN users u ON u.id=l.subject
 WHERE l.subject=p_subject AND l.erased_at IS NULL AND c.contact_verified AND c.contact IS NOT NULL
 AND NOT EXISTS(SELECT 1 FROM moderation_holds h WHERE h.target_kind='account' AND h.target_id IN(c.subject,l.subject)
  AND h.active AND h.effect IN ('restrict','hide','remove') AND h.starts_at<=clock_timestamp() AND (h.ends_at IS NULL OR h.ends_at>clock_timestamp()))
 AND NOT EXISTS(SELECT 1 FROM commercial_learning_restrictions r WHERE r.case_id=c.id AND r.resolved_at IS NULL));
$$;
CREATE FUNCTION commercial_learning_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
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
ALTER FUNCTION commercial_operation(text,uuid,jsonb) RENAME TO commercial_claim_operation;
CREATE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation IN ('learning_start','learning_access','learning_summary','learning_revoke','learning_authority') THEN
  RETURN commercial_learning_operation(p_operation,p_actor,p_body);
 END IF;
 RETURN commercial_claim_operation(p_operation,p_actor,p_body);
END $$;
