-- A commercial case is independent of ordinary account and moderation lifecycle.
-- These are retained original facts, not a payment, waiver or new cash concession.
CREATE TABLE commercial_cases (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), subject uuid NOT NULL UNIQUE,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), erased_at timestamptz,
 contact text, contact_verified boolean NOT NULL DEFAULT false,
 contact_provenance jsonb NOT NULL DEFAULT '{}'::jsonb, contact_epoch bigint NOT NULL DEFAULT 1,
 access_epoch bigint NOT NULL DEFAULT 1,
 inventory jsonb NOT NULL DEFAULT '{"backend":"pending","events":"pending","skills":"pending","challenges":"pending"}',
 review_due_at timestamptz NOT NULL DEFAULT clock_timestamp(), assigned_to uuid,
 review_reason text NOT NULL DEFAULT 'Determine surviving rights and necessary evidence',
 preserve_period_evidence boolean NOT NULL DEFAULT true, closed_at timestamptz
);
-- Receipt is committed independently of later erasure. There is deliberately no
-- account/case FK whose lock could couple intake to the transaction being erased.
CREATE TABLE commercial_erasure_intake (
 subject uuid PRIMARY KEY, id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
 received_at timestamptz NOT NULL, recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 source text NOT NULL DEFAULT 'authenticated_service_receipt' CHECK(source='authenticated_service_receipt')
);
CREATE TRIGGER immutable_commercial_erasure_intake BEFORE UPDATE ON commercial_erasure_intake
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();

CREATE TABLE commercial_requests (
 id uuid PRIMARY KEY, case_id uuid NOT NULL REFERENCES commercial_cases(id),
 kind text NOT NULL, source text NOT NULL, received_at timestamptz,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(), evidence jsonb NOT NULL,
 CHECK(source IN ('authenticated_service_receipt','verified_earlier_declaration','database_erasure_observed','legacy_evidence','claimant'))
);
CREATE TRIGGER immutable_commercial_request BEFORE UPDATE ON commercial_requests
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE UNIQUE INDEX commercial_erasure_request ON commercial_requests(case_id) WHERE kind='account_erasure';
CREATE TABLE commercial_evidence (
 id uuid PRIMARY KEY DEFAULT gen_random_uuid(), case_id uuid NOT NULL REFERENCES commercial_cases(id),
 category text NOT NULL, source_key text NOT NULL, evidence jsonb NOT NULL,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(case_id,category,source_key)
);
CREATE TRIGGER immutable_commercial_evidence BEFORE UPDATE ON commercial_evidence
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE commercial_obligations (
 id uuid PRIMARY KEY, case_id uuid NOT NULL REFERENCES commercial_cases(id),
 source text NOT NULL, source_key text NOT NULL, component text NOT NULL,
 units bigint CHECK(units>=0), cash_units bigint CHECK(cash_units>=0 AND cash_units<=units),
 status text NOT NULL CHECK(status IN ('pending_evidence','established','historical_wallet_application','rejected')),
 original jsonb NOT NULL, determination jsonb,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 review_due_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(source,source_key,component,case_id)
);
CREATE TABLE commercial_operation_aliases (
 operation_id uuid PRIMARY KEY, obligation_id uuid NOT NULL REFERENCES commercial_obligations(id),
 original_payload jsonb NOT NULL, recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable_commercial_alias BEFORE UPDATE ON commercial_operation_aliases
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE commercial_source_components (
 source text NOT NULL, source_id uuid NOT NULL, component text NOT NULL,
 case_id uuid NOT NULL REFERENCES commercial_cases(id), obligation_id uuid NOT NULL REFERENCES commercial_obligations(id),
 PRIMARY KEY(source,source_id,component,case_id)
);
CREATE TRIGGER immutable_commercial_source_component BEFORE UPDATE ON commercial_source_components
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE commercial_journal (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 case_id uuid REFERENCES commercial_cases(id), obligation_id uuid REFERENCES commercial_obligations(id),
 actor uuid, command_id uuid NOT NULL UNIQUE, kind text NOT NULL, request jsonb NOT NULL, result jsonb NOT NULL,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable_commercial_journal BEFORE UPDATE ON commercial_journal
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE commercial_reservations (
 id uuid PRIMARY KEY, obligation_id uuid NOT NULL REFERENCES commercial_obligations(id),
 units bigint NOT NULL CHECK(units>0), mode text NOT NULL CHECK(mode IN ('cash','wallet','redemption')),
 state text NOT NULL CHECK(state IN ('reserved','uncertain','completed','failed')),
 request jsonb NOT NULL, outcome jsonb,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), completed_at timestamptz,
 external_reference text UNIQUE
);
CREATE TABLE commercial_access_keys (
 hash text PRIMARY KEY CHECK(length(hash)=64), case_id uuid NOT NULL REFERENCES commercial_cases(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), revoked_at timestamptz, epoch bigint NOT NULL,
 provenance jsonb NOT NULL
);
CREATE TABLE commercial_document_holds (
 case_id uuid NOT NULL REFERENCES commercial_cases(id), number text NOT NULL REFERENCES financial_documents(number),
 basis text NOT NULL, review_due_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(case_id,number)
);
CREATE TABLE commercial_contract_holds (
 case_id uuid NOT NULL REFERENCES commercial_cases(id), declaration_id uuid NOT NULL REFERENCES contract_declarations(id),
 basis text NOT NULL, review_due_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY(case_id,declaration_id)
);
CREATE TABLE commercial_renewal_holds (
 case_id uuid NOT NULL REFERENCES commercial_cases(id), agreement_id uuid NOT NULL REFERENCES premium_renewal_agreements(id),
 basis text NOT NULL, review_due_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY(case_id,agreement_id)
);
CREATE TABLE commercial_legacy_renewal_holds (
 case_id uuid NOT NULL REFERENCES commercial_cases(id), user_id uuid NOT NULL REFERENCES premium_legacy_renewals(user_id),
 basis text NOT NULL, review_due_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY(case_id,user_id)
);
CREATE TABLE commercial_archive_work (
 number text NOT NULL, kind text NOT NULL CHECK(kind IN ('invoice','credit_note','final_statement')),
 source text NOT NULL CHECK(source IN ('record_disposal','unrecorded_archive')),
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(), review_due_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 disposal_authorized boolean NOT NULL DEFAULT false, assessment jsonb,
 disposal_started_at timestamptz, file_removed_at timestamptz,
 PRIMARY KEY(number,kind)
);
-- Minimum number/subject association for separately retained review/disposal
-- evidence. No account recreation or payment/contract ownership inference.
CREATE TABLE commercial_retention_owners (
 number text NOT NULL, kind text NOT NULL, subject uuid NOT NULL,
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp(), review_due_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 source text NOT NULL, PRIMARY KEY(number,kind,subject)
);
CREATE TRIGGER immutable_commercial_retention_owner BEFORE UPDATE ON commercial_retention_owners
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE FUNCTION commercial_capture_retention_owner(p_number text,p_kind text,p_owner uuid DEFAULT NULL) RETURNS void LANGUAGE plpgsql AS $$
DECLARE owner_id uuid; known_owners uuid[];
BEGIN
 SELECT user_id INTO owner_id FROM financial_documents WHERE number=p_number AND kind=p_kind;
 owner_id:=coalesce(owner_id,p_owner);
 IF owner_id IS NULL THEN
  SELECT array_agg(DISTINCT subject) INTO known_owners FROM moderation_retained_record_owners WHERE kind='financial_document' AND record_id=p_number;
  IF cardinality(known_owners)=1 THEN owner_id:=known_owners[1]; END IF;
 END IF;
 IF owner_id IS NULL AND p_kind='invoice' THEN
  SELECT user_id INTO owner_id FROM paypal_payments WHERE 'R'||lpad(invoice_number::text,7,'0')=p_number;
  IF owner_id IS NULL THEN SELECT user_id INTO owner_id FROM paypal_coin_orders WHERE 'R'||lpad(invoice_number::text,7,'0')=p_number; END IF;
 END IF;
 IF owner_id IS NOT NULL THEN INSERT INTO commercial_retention_owners(number,kind,subject,source)
  VALUES(p_number,p_kind,owner_id,'observed_original_record_owner') ON CONFLICT DO NOTHING; END IF;
END $$;
CREATE FUNCTION commercial_retention_owned(p_subject uuid,p_number text,p_kind text) RETURNS boolean LANGUAGE sql AS $$
 SELECT coalesce((SELECT d.user_id=p_subject FROM financial_documents d WHERE d.number=p_number AND d.kind=p_kind AND d.user_id IS NOT NULL),
  (SELECT count(DISTINCT o.subject)=1 AND bool_and(o.subject=p_subject) FROM commercial_retention_owners o WHERE o.number=p_number AND o.kind=p_kind),false);
$$;
-- An old statement timestamp is not proof that its underlying claim vanished.
-- Every statement requires an individual disposal assessment before pruning.
CREATE TABLE commercial_statement_disposal_reviews (
 number text PRIMARY KEY REFERENCES financial_documents(number) ON DELETE CASCADE,
 review_due_at timestamptz NOT NULL DEFAULT clock_timestamp(), authorized boolean NOT NULL DEFAULT false,
 assessment jsonb
);
INSERT INTO commercial_statement_disposal_reviews(number)
 SELECT number FROM financial_documents WHERE kind='final_statement';
CREATE FUNCTION commercial_statement_review() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM commercial_capture_retention_owner(NEW.number,NEW.kind,NEW.user_id);
 IF NEW.kind='final_statement' THEN INSERT INTO commercial_statement_disposal_reviews(number) VALUES(NEW.number) ON CONFLICT DO NOTHING; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_statement_review AFTER INSERT ON financial_documents
 FOR EACH ROW EXECUTE FUNCTION commercial_statement_review();
CREATE FUNCTION commercial_archive_disposal_queue() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM commercial_capture_retention_owner(OLD.number,OLD.kind,OLD.user_id);
 IF OLD.kind='invoice' THEN
  DELETE FROM invoice_originals WHERE invoice_number=OLD.number;
  DELETE FROM invoice_reconciliation WHERE invoice_number=OLD.number;
 END IF;
 INSERT INTO commercial_archive_work(number,kind,source,disposal_authorized,assessment)
 VALUES(OLD.number,OLD.kind,'record_disposal',true,jsonb_build_object('basis','record_retention_elapsed_and_no_remaining_assessed_hold','record_issued_at',OLD.issued_at))
 ON CONFLICT(number,kind) DO UPDATE SET source='record_disposal',disposal_authorized=true,assessment=EXCLUDED.assessment;
 RETURN OLD;
END $$;
CREATE TRIGGER commercial_archive_disposal_queue AFTER DELETE ON financial_documents
 FOR EACH ROW EXECUTE FUNCTION commercial_archive_disposal_queue();

-- Record adoption and disposal admission serialize by the exact archive number.
-- The committed intent remains after a file error; a retired identifier cannot
-- be adopted while removal is uncertain or after it has completed.
CREATE FUNCTION commercial_archive_record_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-archive:'||NEW.number,0));
 IF EXISTS(SELECT 1 FROM commercial_archive_work WHERE number=NEW.number AND (source='record_disposal' OR disposal_started_at IS NOT NULL)) THEN
  RAISE EXCEPTION 'Retired original identifier; use an explicitly linked new correction document';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_archive_record_guard BEFORE INSERT ON financial_documents
 FOR EACH ROW EXECUTE FUNCTION commercial_archive_record_guard();
CREATE FUNCTION commercial_preserve_records(p_case uuid,p_subject uuid) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO commercial_document_holds(case_id,number,basis)
 SELECT p_case,d.number,'Independent open commercial rights: assess necessary original evidence'
 FROM financial_documents d WHERE d.user_id=p_subject OR EXISTS(SELECT 1 FROM moderation_retained_record_owners o WHERE o.subject=p_subject AND o.kind='financial_document' AND o.record_id=d.number) ON CONFLICT DO NOTHING;
 INSERT INTO commercial_contract_holds(case_id,declaration_id,basis)
 SELECT p_case,d.id,'Independent open commercial rights: original declaration and cascade evidence'
 FROM contract_declarations d WHERE d.user_id=p_subject OR EXISTS(SELECT 1 FROM moderation_retained_record_owners o WHERE o.subject=p_subject AND o.kind='contract_declaration' AND o.record_id=d.id::text) ON CONFLICT DO NOTHING;
 INSERT INTO commercial_renewal_holds(case_id,agreement_id,basis)
 SELECT p_case,id,'Independent paid-period and renewal evidence review' FROM premium_renewal_agreements WHERE user_id=p_subject ON CONFLICT DO NOTHING;
 INSERT INTO commercial_legacy_renewal_holds(case_id,user_id,basis)
 SELECT p_case,user_id,'Independent original legacy paid-period evidence review' FROM premium_legacy_renewals WHERE user_id=p_subject ON CONFLICT DO NOTHING;
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 SELECT p_case,'premium_operation',p.id::text,to_jsonb(p) FROM premium_period_changes p WHERE p.user_id=p_subject
 AND NOT EXISTS(SELECT 1 FROM contract_premium_operations d WHERE d.operation_id=p.id) ON CONFLICT DO NOTHING;
END $$;

-- Unrecorded database originals have no invented issue date/retention deadline.
-- They enter the same individual archive review lane as unrecorded files.
INSERT INTO commercial_archive_work(number,kind,source)
 SELECT i.invoice_number,'invoice','unrecorded_archive' FROM invoice_originals i WHERE NOT EXISTS(SELECT 1 FROM financial_documents d WHERE d.number=i.invoice_number) ON CONFLICT DO NOTHING;
SELECT commercial_capture_retention_owner(number,kind,user_id) FROM financial_documents;
CREATE FUNCTION commercial_capture_period_evidence() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 SELECT c.id,'premium_operation',NEW.id::text,to_jsonb(NEW) FROM commercial_cases c WHERE c.subject=NEW.user_id AND c.preserve_period_evidence ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER commercial_capture_period_evidence AFTER INSERT ON premium_period_changes
 FOR EACH ROW EXECUTE FUNCTION commercial_capture_period_evidence();

CREATE TABLE commercial_disposals (
 id uuid PRIMARY KEY, case_id uuid NOT NULL, actor uuid NOT NULL,
 category text NOT NULL, assessment jsonb NOT NULL,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER immutable_commercial_disposal BEFORE UPDATE ON commercial_disposals
 FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();

-- Call before case/obligation/wallet mutation. A waiter sees atomic account erasure
-- and its already-created case. No operation/payment lock is acquired here.
CREATE FUNCTION commercial_lock_subject(p_subject uuid,p_evidenced_absence boolean DEFAULT false) RETURNS uuid LANGUAGE plpgsql AS $$
DECLARE result uuid; account users;
BEGIN
 SELECT * INTO account FROM users WHERE id=p_subject FOR UPDATE;
 IF FOUND THEN
  INSERT INTO commercial_cases(subject,contact,contact_verified,contact_provenance)
  VALUES(p_subject,account.email,account.email_verified,jsonb_build_object('kind','live_account','observed_at',clock_timestamp(),'verified',account.email_verified))
  ON CONFLICT(subject) DO NOTHING;
 ELSIF p_evidenced_absence THEN
  INSERT INTO commercial_cases(subject,review_reason,inventory)
  VALUES(p_subject,'Original Events financial evidence; historic account/erasure facts require review',
   '{"backend":"unknown","events":"preserved","skills":"unknown","challenges":"unknown"}'::jsonb)
  ON CONFLICT(subject) DO NOTHING;
 END IF;
 SELECT id INTO result FROM commercial_cases WHERE subject=p_subject FOR UPDATE;
 IF result IS NULL THEN RAISE EXCEPTION 'No evidenced commercial recipient'; END IF;
 RETURN result;
END $$;

CREATE FUNCTION commercial_erasure_envelope() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE c uuid; request_id uuid:=gen_random_uuid(); receipt timestamptz;
 wallet coins; captured bigint; provenance text; snapshot jsonb;
BEGIN
 -- The existing moderation self-erasure guard remains authoritative. This trigger
 -- supplies preservation even to an authorized database path, not deletion authority.
 IF current_setting('academy.moderation_erasure_subject',true) IS DISTINCT FROM OLD.id::text THEN
  RAISE EXCEPTION 'Authenticated self-erasure context required';
 END IF;
 c:=commercial_lock_subject(OLD.id);
 SELECT * INTO wallet FROM coins WHERE user_id=OLD.id FOR UPDATE;
 SELECT coalesce(sum(coins),0) INTO captured FROM paypal_coin_orders WHERE user_id=OLD.id AND captured_at IS NOT NULL;
 SELECT i.id,i.received_at INTO request_id,receipt FROM commercial_erasure_intake i WHERE i.subject=OLD.id;
 IF NOT FOUND THEN request_id:=gen_random_uuid(); END IF;
 provenance:=CASE WHEN receipt IS NULL THEN 'database_erasure_observed' ELSE 'authenticated_service_receipt' END;
 INSERT INTO commercial_requests(id,case_id,kind,source,received_at,evidence)
 VALUES(request_id,c,'account_erasure',provenance,receipt,jsonb_build_object('observed_deletion_at',clock_timestamp(),'scope','account_erasure','earlier_declaration','not_inferred'))
 ON CONFLICT DO NOTHING;
 UPDATE commercial_cases SET erased_at=clock_timestamp(),closed_at=NULL,review_due_at=least(review_due_at,clock_timestamp()),
  inventory=inventory||'{"backend":"preserved","events":"pending","skills":"pending","challenges":"pending"}'::jsonb
 WHERE id=c;
 snapshot:=jsonb_build_object('available',coalesce(wallet.coins,0),'withheld',coalesce(wallet.withheld_coins,0),
  'captured_purchase_units',captured,'prior_cash_refunds','unknown','policy','fungible_reward_first',
  'unused_purchase_upper_bound',least(coalesce(wallet.coins,0),captured),'not_a_final_liability_total',true);
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence) VALUES(c,'wallet_boundary',OLD.id::text,snapshot) ON CONFLICT DO NOTHING;
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 SELECT c,'legacy_purchase',id,to_jsonb(o) FROM paypal_coin_orders o WHERE user_id=OLD.id ON CONFLICT DO NOTHING;
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 SELECT c,'ledger',id::text,to_jsonb(t) FROM transactions t WHERE user_id=OLD.id ON CONFLICT DO NOTHING;
 INSERT INTO commercial_evidence(case_id,category,source_key,evidence)
 SELECT c,'premium_operation',id::text,to_jsonb(p) FROM premium_period_changes p WHERE user_id=OLD.id
 AND NOT EXISTS(SELECT 1 FROM contract_premium_operations d WHERE d.operation_id=p.id) ON CONFLICT DO NOTHING;
 INSERT INTO commercial_obligations(id,case_id,source,source_key,component,units,cash_units,status,original)
 VALUES(gen_random_uuid(),c,'backend',OLD.id::text,'wallet_available',coalesce(wallet.coins,0),CASE WHEN captured=0 THEN 0 ELSE NULL END,
  'established',snapshot) ON CONFLICT DO NOTHING;
 INSERT INTO commercial_obligations(id,case_id,source,source_key,component,units,status,original)
 VALUES(gen_random_uuid(),c,'backend',OLD.id::text,'wallet_withheld',coalesce(wallet.withheld_coins,0),'pending_evidence',snapshot) ON CONFLICT DO NOTHING;
 PERFORM commercial_preserve_records(c,OLD.id);
 RETURN OLD;
END $$;
CREATE TRIGGER commercial_erasure_envelope BEFORE DELETE ON users
 FOR EACH ROW EXECUTE FUNCTION commercial_erasure_envelope();

-- Existing queue timestamps are imported only as transaction/observed evidence.
-- They are not silently promoted to a prior customer declaration.
INSERT INTO commercial_cases(subject,erased_at,review_reason)
 SELECT user_id,min(requested_at),'Legacy erasure inventory and original receipt require review'
 FROM user_deletion_work GROUP BY user_id ON CONFLICT DO NOTHING;
INSERT INTO commercial_requests(id,case_id,kind,source,evidence)
 SELECT gen_random_uuid(),c.id,'account_erasure','legacy_evidence',jsonb_build_object('queue_transaction_time',min(w.requested_at),'customer_receipt','unknown')
 FROM commercial_cases c JOIN user_deletion_work w ON w.user_id=c.subject GROUP BY c.id ON CONFLICT DO NOTHING;

CREATE FUNCTION commercial_retention_export(p_subject uuid) RETURNS jsonb LANGUAGE sql AS $$
 SELECT jsonb_build_object(
 'statement_reviews',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM commercial_statement_disposal_reviews r WHERE commercial_retention_owned(p_subject,r.number,'final_statement')),'[]'),
 'archive_work',coalesce((SELECT jsonb_agg(to_jsonb(w)) FROM commercial_archive_work w WHERE commercial_retention_owned(p_subject,w.number,w.kind)),'[]'),
 'history',coalesce((SELECT jsonb_agg(to_jsonb(j) ORDER BY j.id) FROM commercial_journal j WHERE j.case_id IS NULL AND j.kind IN ('statement_review','archive_review')
 AND commercial_retention_owned(p_subject,j.request->>'number',CASE WHEN j.kind='statement_review' THEN 'final_statement' ELSE j.request->>'kind' END)),'[]'),
 'owner_associations',coalesce((SELECT jsonb_agg(to_jsonb(o)) FROM commercial_retention_owners o WHERE o.subject=p_subject AND commercial_retention_owned(p_subject,o.number,o.kind)),'[]'),
 'scope','Existing number-linked records with established owner authority; unknown historical ownership is not inferred');
$$;

CREATE FUNCTION commercial_case_export(p_subject uuid) RETURNS jsonb LANGUAGE sql AS $$
 SELECT coalesce(CASE WHEN c.id IS NULL THEN (SELECT jsonb_build_object('case',NULL,'erasure_intake',to_jsonb(i),'financial_inventory','pending') FROM commercial_erasure_intake i WHERE i.subject=p_subject) ELSE jsonb_build_object(
 'case',to_jsonb(c),'erasure_intake',(SELECT to_jsonb(i) FROM commercial_erasure_intake i WHERE i.subject=p_subject),'requests',coalesce((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.recorded_at,r.id) FROM commercial_requests r WHERE r.case_id=c.id),'[]'),
 'obligations',coalesce((SELECT jsonb_agg(to_jsonb(o) ORDER BY o.created_at,o.id) FROM commercial_obligations o WHERE o.case_id=c.id),'[]'),
 'evidence',coalesce((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.recorded_at,e.id) FROM commercial_evidence e WHERE e.case_id=c.id),'[]'),
 'journal',coalesce((SELECT jsonb_agg(to_jsonb(j)||jsonb_build_object('request',j.request-'hash') ORDER BY j.id) FROM commercial_journal j WHERE j.case_id=c.id),'[]'),
 'reservations',coalesce((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.created_at,r.id) FROM commercial_reservations r JOIN commercial_obligations o ON o.id=r.obligation_id WHERE o.case_id=c.id),'[]'),
 'document_holds',coalesce((SELECT jsonb_agg(to_jsonb(h) ORDER BY h.number) FROM commercial_document_holds h WHERE h.case_id=c.id),'[]'),
 'contract_holds',coalesce((SELECT jsonb_agg(to_jsonb(h)) FROM commercial_contract_holds h WHERE h.case_id=c.id),'[]'),
 'renewal_holds',coalesce((SELECT jsonb_agg(to_jsonb(h)) FROM commercial_renewal_holds h WHERE h.case_id=c.id),'[]'),
 'legacy_renewal_holds',coalesce((SELECT jsonb_agg(to_jsonb(h)) FROM commercial_legacy_renewal_holds h WHERE h.case_id=c.id),'[]'),
 'disposals',coalesce((SELECT jsonb_agg(to_jsonb(d) ORDER BY d.recorded_at,d.id) FROM commercial_disposals d WHERE d.case_id=c.id),'[]')) END
 ,'{}'::jsonb)||jsonb_build_object('retention_reviews',commercial_retention_export(p_subject))
 FROM (SELECT 1) sentinel LEFT JOIN commercial_cases c ON c.subject=p_subject;
$$;

CREATE FUNCTION commercial_remaining(p_obligation uuid) RETURNS bigint LANGUAGE sql AS $$
 SELECT CASE WHEN o.status IN ('historical_wallet_application','rejected') THEN 0 ELSE greatest(0,o.units-coalesce((SELECT sum(r.units) FROM commercial_reservations r
 WHERE r.obligation_id=o.id AND r.state IN ('reserved','uncertain','completed')),0)) END FROM commercial_obligations o WHERE o.id=p_obligation;
$$;

-- The application separately authenticates fixed recipient, staff and internal
-- routes. Actor/subject are never taken from a recipient's untrusted JSON body.
CREATE FUNCTION commercial_staff_active(p_actor uuid,p_body jsonb) RETURNS boolean LANGUAGE sql AS $$
 SELECT EXISTS(SELECT 1 FROM users u JOIN sessions s ON s.user_id=u.id
  JOIN session_refresh_tokens r ON r.session_id=s.id
  WHERE u.id=p_actor AND u.enabled AND u.admin AND s.mfa_verified
  AND s.id=(p_body->>'_staff_session')::uuid
  AND encode(r.refresh_token_hash,'hex')=p_body->>'_staff_refresh_hash');
$$;

CREATE FUNCTION commercial_retention_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE command uuid; old_command commercial_journal; request_body jsonb; result jsonb;
BEGIN
 IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Current administrator MFA authority required'; END IF;
 command:=(p_body->>'command_id')::uuid;
 IF command IS NULL OR length(coalesce(p_body->>'assessment',''))<20 OR nullif(p_body->>'next_review_at','')::timestamptz IS NULL THEN RAISE EXCEPTION 'Exact review identity, necessity assessment and next review required'; END IF;
 request_body:=p_body-'_staff_session'-'_staff_refresh_hash'-'_claim_hash'-'_moderation_hash';
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
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
  PERFORM 1 FROM commercial_archive_work WHERE number=p_body->>'number' AND kind=p_body->>'kind' AND source='unrecorded_archive' AND disposal_started_at IS NULL AND file_removed_at IS NULL FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'Pending unrecorded archive required'; END IF;
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

CREATE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE c uuid; owner_id uuid; command uuid; old_command commercial_journal;
 obligation commercial_obligations; reservation commercial_reservations; old_operation internal_coin_operations;
 result jsonb; request_body jsonb; identity jsonb; observation jsonb; amount bigint; cash bigint; remaining bigint;
 op_id uuid; v_source_id uuid; target_state text;
BEGIN
 IF p_body IS NULL OR jsonb_typeof(p_body)<>'object' THEN RAISE EXCEPTION 'Object request required'; END IF;
 request_body:=p_body-'_claim_hash'-'_moderation_hash'-'_staff_session'-'_staff_refresh_hash';
 IF p_operation='authenticate' THEN
  RETURN (SELECT jsonb_build_object('subject',x.subject,'case_id',x.id)
   FROM commercial_access_keys a JOIN commercial_cases x ON x.id=a.case_id
   WHERE a.hash=p_body->>'hash' AND a.revoked_at IS NULL AND a.epoch=x.access_epoch);
 END IF;
 IF p_operation='erasure' THEN
  RETURN (SELECT jsonb_build_object('protocol',1,'case_id',x.id,'subject',x.subject,'erased_at',x.erased_at,'request',to_jsonb(r))
   FROM commercial_cases x JOIN commercial_requests r ON r.case_id=x.id AND r.kind='account_erasure'
   WHERE x.subject=p_actor AND x.erased_at IS NOT NULL);
 END IF;
 IF p_operation='export' THEN RETURN commercial_case_export(p_actor); END IF;
 IF p_operation='retention_queue' THEN
  RETURN jsonb_build_object('statements',coalesce((SELECT jsonb_agg(q) FROM (SELECT r.*,d.settled_at AS historical_staff_assertion,d.issued_at FROM commercial_statement_disposal_reviews r JOIN financial_documents d ON d.number=r.number ORDER BY r.review_due_at,r.number LIMIT 100)q),'[]'),
   'archives',coalesce((SELECT jsonb_agg(q) FROM (SELECT * FROM commercial_archive_work WHERE file_removed_at IS NULL ORDER BY review_due_at,number LIMIT 100)q),'[]'),
   'retained_owner_associations',coalesce((SELECT jsonb_agg(q) FROM (SELECT * FROM commercial_retention_owners ORDER BY review_due_at,number LIMIT 100)q),'[]'));
 END IF;
 IF p_operation IN ('statement_review','archive_review') THEN RETURN commercial_retention_operation(p_operation,p_actor,p_body); END IF;
 IF p_operation='queue' THEN
  RETURN coalesce((SELECT jsonb_agg(q) FROM (
   SELECT x.id,x.subject,x.erased_at,x.inventory,x.assigned_to,x.review_reason,x.closed_at,
    least(x.review_due_at,(SELECT min(o.review_due_at) FROM commercial_obligations o WHERE o.case_id=x.id AND (o.status='pending_evidence' OR commercial_remaining(o.id)>0)),
     (SELECT min(h.review_due_at) FROM commercial_document_holds h WHERE h.case_id=x.id),
     (SELECT min(h.review_due_at) FROM commercial_contract_holds h WHERE h.case_id=x.id),
     (SELECT min(h.review_due_at) FROM commercial_renewal_holds h WHERE h.case_id=x.id),
     (SELECT min(h.review_due_at) FROM commercial_legacy_renewal_holds h WHERE h.case_id=x.id)) AS due_at
   FROM commercial_cases x ORDER BY due_at,x.created_at,x.id LIMIT 100 OFFSET greatest(0,coalesce((p_body->>'offset')::integer,0))
  )q),'[]');
 END IF;
 IF p_operation NOT IN ('open','access','revoke','inventory','register_event','determine','elect','reserve','outcome','review','minimize_contact','release_document','release_record') THEN
  RAISE EXCEPTION 'Unsupported commercial operation';
 END IF;
 IF jsonb_typeof(p_body->'command_id') IS DISTINCT FROM 'string' THEN RAISE EXCEPTION 'Exact command identity required'; END IF;
 command:=(p_body->>'command_id')::uuid;
 IF p_actor IS NULL THEN RAISE EXCEPTION 'Proved actor required'; END IF;
 -- Type checks precede exact replay: JSON numbers/objects never impersonate text.
 IF p_operation IN ('determine','reserve','outcome','review','minimize_contact','release_document','release_record') THEN
  IF jsonb_typeof(p_body->'assessment') IS DISTINCT FROM 'string' OR length(trim(p_body->>'assessment'))<20 THEN
   RAISE EXCEPTION 'Specific human assessment required';
  END IF;
 END IF;
 IF p_operation IN ('register_event','inventory') THEN owner_id:=(p_body->>'subject')::uuid;
 ELSIF p_operation IN ('determine','reserve','outcome','review','minimize_contact','release_document','release_record') THEN
  SELECT subject INTO owner_id FROM commercial_cases WHERE id=(p_body->>'case_id')::uuid;
 ELSE owner_id:=p_actor; END IF;
 IF owner_id IS NULL THEN RAISE EXCEPTION 'Commercial recipient required'; END IF;
 -- All callers acquire this request lock before the user/case prerequisite. An
 -- Events alias uses the same lock as the compatible T6 writer before user lookup.
 PERFORM pg_advisory_xact_lock(hashtextextended('commercial-command:'||command,0));
 IF p_operation IN ('determine','reserve','outcome','review','minimize_contact','release_document','release_record')
  AND NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Current administrator MFA authority required'; END IF;
 SELECT * INTO old_command FROM commercial_journal WHERE command_id=command;
 IF FOUND THEN
  IF old_command.actor IS DISTINCT FROM p_actor OR old_command.kind<>p_operation OR old_command.request<>request_body THEN RAISE EXCEPTION 'Conflicting commercial command replay'; END IF;
  RETURN old_command.result;
 END IF;
 IF p_operation='register_event' THEN
  op_id:=nullif(p_body->>'operation_id','')::uuid;
  IF op_id IS NOT NULL THEN PERFORM pg_advisory_xact_lock(hashtextextended('coin-operation:'||op_id,0)); END IF;
 END IF;
 IF p_operation='register_event' AND (jsonb_typeof(p_body->'identity') IS DISTINCT FROM 'object'
  OR jsonb_typeof(p_body->'observation') IS DISTINCT FROM 'object'
  OR jsonb_typeof(p_body->'identity'->'payment_ids') IS DISTINCT FROM 'array'
  OR jsonb_array_length(p_body->'identity'->'payment_ids')=0) THEN RAISE EXCEPTION 'Original Events evidence required before recipient adoption'; END IF;
 c:=commercial_lock_subject(owner_id,p_operation='register_event');
 IF p_operation IN ('determine','reserve','outcome','review','minimize_contact','release_document','release_record')
  AND NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Administrator authority changed while waiting'; END IF;
 IF p_body ? 'case_id' AND (p_body->>'case_id')::uuid<>c THEN RAISE EXCEPTION 'Recipient case mismatch'; END IF;
 IF p_operation IN ('access','elect','revoke') THEN
  IF p_body ? '_claim_hash' THEN
   IF NOT EXISTS(SELECT 1 FROM commercial_access_keys a JOIN commercial_cases x ON x.id=a.case_id
    WHERE a.hash=p_body->>'_claim_hash' AND a.case_id=c AND a.revoked_at IS NULL AND a.epoch=x.access_epoch) THEN RAISE EXCEPTION 'Claim authority changed'; END IF;
  ELSIF p_body ? '_moderation_hash' THEN
   IF NOT EXISTS(SELECT 1 FROM moderation_capabilities WHERE hash=p_body->>'_moderation_hash' AND subject=owner_id AND scope='rights' AND revoked_at IS NULL AND expires_at>clock_timestamp()) THEN RAISE EXCEPTION 'Original recipient authority changed'; END IF;
  ELSE RAISE EXCEPTION 'Fresh personal recipient proof required'; END IF;
 END IF;
 IF p_operation='open' THEN
  PERFORM commercial_preserve_records(c,owner_id);
  result:=jsonb_build_object('case_id',c,'status','inventory_pending');
 ELSIF p_operation='access' THEN
  IF jsonb_typeof(p_body->'hash') IS DISTINCT FROM 'string' OR p_body->>'hash' !~ '^[0-9a-f]{64}$' THEN RAISE EXCEPTION 'Opaque credential hash required'; END IF;
  UPDATE commercial_cases SET access_epoch=access_epoch+1 WHERE id=c;
  UPDATE commercial_access_keys SET revoked_at=clock_timestamp() WHERE case_id=c AND revoked_at IS NULL;
  INSERT INTO commercial_access_keys(hash,case_id,epoch,provenance)
  SELECT p_body->>'hash',c,access_epoch,jsonb_build_object('kind','explicit_proved_recipient_rotation','actor',p_actor) FROM commercial_cases WHERE id=c;
  result:=jsonb_build_object('case_id',c,'claim_value_expires',false);
 ELSIF p_operation='revoke' THEN
  UPDATE commercial_access_keys SET revoked_at=clock_timestamp() WHERE case_id=c AND hash=p_body->>'hash' AND revoked_at IS NULL;
  result:=jsonb_build_object('revoked',true);
 ELSIF p_operation='inventory' THEN
  IF p_body->>'service' NOT IN ('events','skills','challenges') OR p_body->>'state' NOT IN ('preserved','pending','erased') THEN RAISE EXCEPTION 'Supported inventory state required'; END IF;
  UPDATE commercial_cases SET inventory=inventory||jsonb_build_object(p_body->>'service',p_body->>'state'),closed_at=NULL WHERE id=c;
  result:=jsonb_build_object('accepted',true,'financial_satisfaction',false);
 ELSIF p_operation='register_event' THEN
  PERFORM commercial_preserve_records(c,owner_id);
  identity:=p_body->'identity'; observation:=p_body->'observation';
  IF jsonb_typeof(identity) IS DISTINCT FROM 'object' OR jsonb_typeof(observation) IS DISTINCT FROM 'object'
   OR jsonb_typeof(identity->'source_key') IS DISTINCT FROM 'string' OR length(identity->>'source_key')<1
   OR jsonb_typeof(identity->'component') IS DISTINCT FROM 'string'
   OR identity->>'component' NOT IN ('student_refund','instructor_remuneration')
   OR jsonb_typeof(identity->'payment_ids') IS DISTINCT FROM 'array'
   OR jsonb_array_length(identity->'payment_ids')=0
   OR coalesce(jsonb_typeof(observation->'units'),'null') NOT IN ('number','null') THEN RAISE EXCEPTION 'Original Events identity and observation required'; END IF;
  amount:=nullif(observation->>'units','')::bigint;
  IF amount<0 THEN RAISE EXCEPTION 'Negative claim amount'; END IF;
  SELECT * INTO obligation FROM commercial_obligations WHERE source='events' AND source_key=identity->>'source_key' AND component=identity->>'component' AND case_id=c FOR UPDATE;
  IF NOT FOUND THEN
   INSERT INTO commercial_obligations(id,case_id,source,source_key,component,units,status,original)
   VALUES((p_body->>'obligation_id')::uuid,c,'events',identity->>'source_key',identity->>'component',amount,
    CASE WHEN amount IS NULL OR observation->>'entitlement' IS DISTINCT FROM 'established' THEN 'pending_evidence' ELSE 'established' END,identity) RETURNING * INTO obligation;
  ELSIF obligation.original<>identity THEN RAISE EXCEPTION 'Original source identity changed';
  ELSIF obligation.units IS NOT NULL AND amount IS NOT NULL AND obligation.units<>amount THEN
   RAISE EXCEPTION 'Changed amount needs an evidenced correction determination';
  ELSIF obligation.units IS NULL AND amount IS NOT NULL THEN
   UPDATE commercial_obligations SET units=amount,status=CASE WHEN observation->>'entitlement'='established' THEN 'established' ELSE 'pending_evidence' END WHERE id=obligation.id RETURNING * INTO obligation;
  END IF;
  FOR v_source_id IN SELECT value::uuid FROM jsonb_array_elements_text(identity->'payment_ids') LOOP
   INSERT INTO commercial_source_components(source,source_id,component,case_id,obligation_id)
   VALUES('events',v_source_id,identity->>'component',c,obligation.id) ON CONFLICT DO NOTHING;
   IF NOT EXISTS(SELECT 1 FROM commercial_source_components s WHERE s.source='events' AND s.source_id=v_source_id AND s.component=identity->>'component' AND s.case_id=c AND s.obligation_id=obligation.id) THEN
    RAISE EXCEPTION 'Original booking component already belongs to another obligation; reconcile without duplicate satisfaction';
   END IF;
  END LOOP;
  IF op_id IS NOT NULL THEN
   IF jsonb_typeof(p_body->'operation_payload') IS DISTINCT FROM 'object' THEN RAISE EXCEPTION 'Exact old operation payload required'; END IF;
   INSERT INTO commercial_operation_aliases(operation_id,obligation_id,original_payload)
   VALUES(op_id,obligation.id,p_body->'operation_payload') ON CONFLICT DO NOTHING;
   IF NOT EXISTS(SELECT 1 FROM commercial_operation_aliases WHERE operation_id=op_id AND obligation_id=obligation.id AND original_payload=p_body->'operation_payload') THEN RAISE EXCEPTION 'Conflicting operation alias'; END IF;
   SELECT * INTO old_operation FROM internal_coin_operations WHERE id=op_id;
   IF FOUND THEN
    IF old_operation.user_id<>owner_id OR old_operation.coins IS DISTINCT FROM (p_body->'operation_payload'->>'coins')::bigint
     OR old_operation.description IS DISTINCT FROM p_body->'operation_payload'->>'description'
     OR old_operation.credit_note IS DISTINCT FROM (p_body->'operation_payload'->>'credit_note')::boolean THEN RAISE EXCEPTION 'Original coin receipt differs'; END IF;
    IF old_operation.completed_at IS NOT NULL THEN
     UPDATE commercial_obligations SET status='historical_wallet_application',determination=jsonb_build_object('kind','exact_original_wallet_receipt','receipt',to_jsonb(old_operation)) WHERE id=obligation.id;
    ELSE
     UPDATE commercial_obligations SET status='pending_evidence',determination=jsonb_build_object('kind','incomplete_original_operation') WHERE id=obligation.id;
    END IF;
   END IF;
  END IF;
  INSERT INTO commercial_evidence(case_id,category,source_key,evidence) VALUES(c,'event_observation',command::text,observation);
  UPDATE commercial_cases SET closed_at=NULL,review_due_at=least(review_due_at,clock_timestamp()) WHERE id=c;
  SELECT status INTO target_state FROM commercial_obligations WHERE id=obligation.id;
  result:=jsonb_build_object('protocol',1,'case_id',c,'obligation_id',obligation.id,'disposition',CASE WHEN target_state='historical_wallet_application' THEN target_state ELSE 'claim_preserved' END,'paid',false);
 ELSIF p_operation='determine' THEN
  SELECT * INTO obligation FROM commercial_obligations WHERE id=(p_body->>'obligation_id')::uuid AND case_id=c FOR UPDATE;
  IF NOT FOUND OR obligation.status='historical_wallet_application' THEN RAISE EXCEPTION 'Original satisfied obligation cannot be rewritten'; END IF;
  amount:=(p_body->>'units')::bigint; cash:=nullif(p_body->>'cash_units','')::bigint;
  IF amount IS NULL OR amount<0 OR cash<0 OR cash>amount OR jsonb_typeof(p_body->'evidence') IS DISTINCT FROM 'object' OR p_body->'evidence'='{}'::jsonb THEN RAISE EXCEPTION 'Amount, tender certainty and evidence required'; END IF;
  IF obligation.units IS NOT NULL AND amount<>obligation.units THEN RAISE EXCEPTION 'Original amount immutable; use a separately linked adjustment'; END IF;
  IF cash IS NOT NULL AND length(coalesce(p_body->'evidence'->>'cash_basis',''))<20 THEN RAISE EXCEPTION 'Specific purchased-tender or other established cash basis required'; END IF;
  UPDATE commercial_obligations SET units=amount,cash_units=cash,status='established',determination=request_body,review_due_at=clock_timestamp() WHERE id=obligation.id;
  result:=jsonb_build_object('obligation_id',obligation.id,'status','established','paid',false);
 ELSIF p_operation='elect' THEN
  SELECT * INTO obligation FROM commercial_obligations WHERE id=(p_body->>'obligation_id')::uuid AND case_id=c FOR UPDATE;
  IF NOT FOUND OR p_body->>'method' NOT IN ('cash','coins') OR jsonb_typeof(p_body->'method') IS DISTINCT FROM 'string' THEN RAISE EXCEPTION 'Recipient obligation and explicit method required'; END IF;
  result:=jsonb_build_object('received',true,'method',p_body->>'method','paid',false,'review_required',obligation.cash_units IS NULL);
 ELSIF p_operation='reserve' THEN
  SELECT * INTO obligation FROM commercial_obligations WHERE id=(p_body->>'obligation_id')::uuid AND case_id=c FOR UPDATE;
  IF NOT FOUND OR obligation.status<>'established' THEN RAISE EXCEPTION 'Established remaining obligation required'; END IF;
  amount:=(p_body->>'units')::bigint; remaining:=commercial_remaining(obligation.id);
  IF amount IS NULL OR amount<=0 OR amount>remaining THEN RAISE EXCEPTION 'Insufficient unreserved entitlement'; END IF;
  IF p_body->>'mode'='cash' THEN
   IF obligation.cash_units IS NULL OR amount>obligation.cash_units-coalesce((SELECT sum(units) FROM commercial_reservations WHERE obligation_id=obligation.id AND mode='cash' AND state IN ('reserved','uncertain','completed')),0)
    OR jsonb_typeof(p_body->'verification') IS DISTINCT FROM 'object'
    OR length(coalesce(p_body->'verification'->>'claimant',''))<20 OR length(coalesce(p_body->'verification'->>'destination',''))<20
    OR length(coalesce(p_body->'verification'->>'election',''))<20 THEN RAISE EXCEPTION 'Established cash component, claimant election and verified destination required'; END IF;
  ELSIF p_body->>'mode' NOT IN ('wallet','redemption') THEN RAISE EXCEPTION 'Supported disposition required'; END IF;
  INSERT INTO commercial_reservations(id,obligation_id,units,mode,state,request)
  VALUES((p_body->>'reservation_id')::uuid,obligation.id,amount,p_body->>'mode','reserved',request_body);
  result:=jsonb_build_object('reservation_id',p_body->>'reservation_id','state','reserved','units',amount,'paid',false);
 ELSIF p_operation='outcome' THEN
  SELECT r.* INTO reservation FROM commercial_reservations r JOIN commercial_obligations o ON o.id=r.obligation_id WHERE r.id=(p_body->>'reservation_id')::uuid AND o.case_id=c FOR UPDATE OF r;
  IF NOT FOUND OR reservation.mode<>'cash' THEN RAISE EXCEPTION 'Existing cash reservation required; service/wallet completion has owning writer'; END IF;
  target_state:=p_body->>'state';
  IF target_state NOT IN ('completed','failed','uncertain') OR reservation.state IN ('completed','failed') THEN RAISE EXCEPTION 'Conflicting terminal or invalid outcome'; END IF;
  IF jsonb_typeof(p_body->'evidence') IS DISTINCT FROM 'object' OR p_body->'evidence'='{}'::jsonb THEN RAISE EXCEPTION 'Actual outcome evidence required'; END IF;
  IF target_state='failed' AND p_body->'evidence'->'definitive_no_payment' IS DISTINCT FROM 'true'::jsonb THEN RAISE EXCEPTION 'Uncertainty cannot release reserved funds'; END IF;
  IF target_state='completed' AND (length(coalesce(p_body->>'external_reference',''))<3 OR (p_body->>'paid_units')::bigint IS DISTINCT FROM reservation.units
   OR p_body->'evidence'->'destination_matches_reservation' IS DISTINCT FROM 'true'::jsonb) THEN RAISE EXCEPTION 'Exact recorded payment and reservation destination required'; END IF;
  UPDATE commercial_reservations SET state=target_state,outcome=request_body,completed_at=CASE WHEN target_state='completed' THEN clock_timestamp() END,
   external_reference=CASE WHEN target_state='completed' THEN p_body->>'external_reference' ELSE external_reference END WHERE id=reservation.id;
  result:=jsonb_build_object('reservation_id',reservation.id,'state',target_state,'evidence_kind','operator_recorded_external_outcome');
 ELSIF p_operation='review' THEN
  IF nullif(p_body->>'next_review_at','')::timestamptz IS NULL OR jsonb_typeof(p_body->'necessary_fields') IS DISTINCT FROM 'object' THEN RAISE EXCEPTION 'Next review and field necessity assessment required'; END IF;
  UPDATE commercial_cases SET review_due_at=(p_body->>'next_review_at')::timestamptz,assigned_to=p_actor,review_reason=p_body->>'assessment',
   preserve_period_evidence=coalesce((p_body->'necessary_fields'->>'period_operations')::boolean,preserve_period_evidence) WHERE id=c;
  result:=jsonb_build_object('review_recorded',true,'claims_satisfied',false);
 ELSIF p_operation='minimize_contact' THEN
  IF length(coalesce(p_body->>'alternative_access',''))<20 THEN RAISE EXCEPTION 'Assess usable remedy access independently of contact'; END IF;
  UPDATE commercial_cases SET contact=NULL,contact_verified=false,contact_epoch=contact_epoch+1,contact_provenance=jsonb_build_object('kind','reviewed_minimization','command_id',command) WHERE id=c;
  INSERT INTO commercial_disposals(id,case_id,actor,category,assessment) VALUES(command,c,p_actor,'contact',request_body);
  result:=jsonb_build_object('contact_removed',true,'claims_satisfied',false);
 ELSIF p_operation='release_record' THEN
  IF length(coalesce(p_body->>'remaining_evidence_basis',''))<20 THEN RAISE EXCEPTION 'Independent remaining financial evidence assessment required'; END IF;
  CASE p_body->>'kind'
   WHEN 'contract_declaration' THEN DELETE FROM commercial_contract_holds WHERE case_id=c AND declaration_id=(p_body->>'record_id')::uuid;
   WHEN 'renewal_agreement' THEN DELETE FROM commercial_renewal_holds WHERE case_id=c AND agreement_id=(p_body->>'record_id')::uuid;
   WHEN 'legacy_renewal' THEN DELETE FROM commercial_legacy_renewal_holds WHERE case_id=c AND user_id=(p_body->>'record_id')::uuid;
   ELSE RAISE EXCEPTION 'Supported original record kind required';
  END CASE;
  INSERT INTO commercial_disposals(id,case_id,actor,category,assessment) VALUES(command,c,p_actor,'record_hold_release',request_body);
  result:=jsonb_build_object('hold_released',true,'record_deleted',false);
 ELSIF p_operation='release_document' THEN
  IF length(coalesce(p_body->>'remaining_evidence_basis',''))<20 THEN RAISE EXCEPTION 'Independent remaining financial evidence assessment required'; END IF;
  DELETE FROM commercial_document_holds WHERE case_id=c AND number=p_body->>'number';
  INSERT INTO commercial_disposals(id,case_id,actor,category,assessment) VALUES(command,c,p_actor,'document_hold_release',request_body);
  result:=jsonb_build_object('hold_released',true,'document_deleted',false);
 END IF;
 INSERT INTO commercial_journal(case_id,obligation_id,actor,command_id,kind,request,result)
 VALUES(c,obligation.id,p_actor,command,p_operation,request_body,result);
 RETURN result;
END $$;
