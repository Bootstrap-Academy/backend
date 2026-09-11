-- IF1: full original identities, qualified retention metadata and explicit
-- pending review. Historical documents, PDFs, declarations and associations
-- are not renamed, reassigned, replaced or retrospectively verified.
CREATE FUNCTION commercial_invoice_number(p_number bigint) RETURNS text
 LANGUAGE sql IMMUTABLE STRICT AS $$
 SELECT CASE WHEN p_number>=0 THEN 'R'||repeat('0',greatest(0,7-length(p_number::text)))||p_number::text END;
$$;
CREATE FUNCTION commercial_invoice_numeric(p_number text) RETURNS bigint
 LANGUAGE plpgsql IMMUTABLE STRICT AS $$
DECLARE n bigint;
BEGIN
 IF p_number !~ '^R[0-9]{7,19}$' THEN RETURN NULL; END IF;
 BEGIN n:=substring(p_number FROM 2)::bigint;
 EXCEPTION WHEN numeric_value_out_of_range THEN RETURN NULL; END;
 IF commercial_invoice_number(n) IS DISTINCT FROM p_number THEN RETURN NULL; END IF;
 RETURN n;
END $$;

-- Current authority in the same precedence as owned_original_number. NULL
-- means no independent current source; an explicit negative is not absence.
CREATE FUNCTION commercial_invoice_owner_evidence(p_number text) RETURNS jsonb
 LANGUAGE plpgsql AS $$
DECLARE n bigint; p record; d record; owners uuid[];
BEGIN
 n:=commercial_invoice_numeric(p_number);
 IF n IS NULL THEN RETURN jsonb_build_object('basis','unsupported_identifier','eligible',false); END IF;
 SELECT user_id,fulfilled_at,order_id INTO p FROM paypal_payments WHERE invoice_number=n;
 IF FOUND THEN RETURN jsonb_build_object('subject',p.user_id,'eligible',p.fulfilled_at IS NOT NULL,'basis','durable_numeric_payment','source_key',p.order_id); END IF;
 SELECT user_id,id INTO p FROM paypal_coin_orders WHERE invoice_number=n;
 IF FOUND THEN RETURN jsonb_build_object('subject',p.user_id,'eligible',true,'basis','legacy_numeric_order','source_key',p.id); END IF;
 SELECT user_id,kind INTO d FROM financial_documents WHERE number=p_number;
 IF NOT FOUND THEN RETURN NULL; END IF;
 IF d.kind<>'invoice' THEN RETURN jsonb_build_object('basis','different_document_kind','eligible',false); END IF;
 IF d.user_id IS NOT NULL THEN RETURN jsonb_build_object('subject',d.user_id,'eligible',true,'basis','live_document_owner','source_key',p_number); END IF;
 SELECT array_agg(DISTINCT subject) INTO owners FROM moderation_retained_record_owners WHERE kind='financial_document' AND record_id=p_number;
 IF cardinality(owners)=1 THEN RETURN jsonb_build_object('subject',owners[1],'eligible',true,'basis','exact_retained_document_owner','source_key',p_number); END IF;
 IF cardinality(owners)>1 THEN RETURN jsonb_build_object('basis','conflicting_retained_document_owners','eligible',false); END IF;
 RETURN NULL;
END $$;
CREATE FUNCTION commercial_invoice_original_owned(p_subject uuid,p_number text) RETURNS boolean
 LANGUAGE sql AS $$
 SELECT coalesce((e->>'subject')::uuid=p_subject AND (e->>'eligible')::boolean,false)
 FROM (SELECT commercial_invoice_owner_evidence(p_number) e) q;
$$;

-- Staff-only attributable observations. These have no cascading document/user
-- FK and copy no PDF, address, consent text or fabricated historical decision.
CREATE TABLE commercial_invoice_identity_reviews (
 number text NOT NULL, reason text NOT NULL, source_key text NOT NULL,
 observed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 disposition text NOT NULL DEFAULT 'pending_review' CHECK(disposition='pending_review'),
 evidence jsonb NOT NULL, PRIMARY KEY(number,reason,source_key)
);
CREATE TRIGGER immutable_commercial_invoice_identity_review BEFORE UPDATE OR DELETE
 ON commercial_invoice_identity_reviews FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();
CREATE TABLE commercial_invoice_owner_observations (
 number text NOT NULL, subject uuid NOT NULL, basis text NOT NULL,
 evidence_hash text NOT NULL, evidence jsonb NOT NULL,
 qualified boolean NOT NULL, observed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(number,subject,basis,evidence_hash)
);
CREATE TRIGGER immutable_commercial_invoice_owner_observation BEFORE UPDATE OR DELETE
 ON commercial_invoice_owner_observations FOR EACH ROW EXECUTE FUNCTION protect_contract_evidence();

-- A surviving eligible source identifies a review candidate, not proof that
-- the old row was produced by either backfill. Erased sources are unknowable.
INSERT INTO commercial_invoice_identity_reviews(number,reason,source_key,evidence)
 SELECT 'R'||left(o.invoice_number::text,7),'historical_long_invoice_backfill_candidate',o.id,
 jsonb_build_object('source','paypal_coin_orders','canonical_number',commercial_invoice_number(o.invoice_number),
  'captured_source_present',o.captured_at IS NOT NULL,'consent_source_present',o.withdrawal_consent_at IS NOT NULL,
  'source_sha256',encode(sha256(convert_to(to_jsonb(o)::text,'UTF8')),'hex'),
  'meaning','Surviving eligible long source; original row/PDF/consent identity requires review, not prefix repair')
 FROM paypal_coin_orders o WHERE o.invoice_number>=10000000
 AND (o.captured_at IS NOT NULL OR o.withdrawal_consent_at IS NOT NULL)
 AND (EXISTS(SELECT 1 FROM financial_documents d WHERE d.kind='invoice' AND d.number='R'||left(o.invoice_number::text,7))
   OR EXISTS(SELECT 1 FROM invoice_originals i WHERE i.invoice_number='R'||left(o.invoice_number::text,7)));

INSERT INTO commercial_invoice_identity_reviews(number,reason,source_key,evidence)
 SELECT d.number,'numeric_recorded_owner_conflict',d.number,
 jsonb_build_object('recorded_subject',d.user_id,'current_numeric_evidence',e,
  'document_sha256',encode(sha256(convert_to(to_jsonb(d)::text,'UTF8')),'hex'))
 FROM financial_documents d CROSS JOIN LATERAL (SELECT commercial_invoice_owner_evidence(d.number) e) q
 WHERE d.kind='invoice' AND d.user_id IS NOT NULL
 AND e->>'basis' IN ('durable_numeric_payment','legacy_numeric_order')
 AND (e->>'subject')::uuid IS DISTINCT FROM d.user_id;

CREATE FUNCTION commercial_invoice_identity_pending(p_number text) RETURNS boolean
 LANGUAGE sql AS $$
 SELECT EXISTS(SELECT 1 FROM commercial_invoice_identity_reviews WHERE number=p_number);
$$;

-- Preserve every old generic invoice association as an unqualified historical
-- observation. Independent exact evidence can still admit the real owner.
INSERT INTO commercial_invoice_owner_observations(number,subject,basis,evidence_hash,evidence,qualified)
 SELECT o.number,o.subject,'legacy_branch_unknown',encode(sha256(convert_to(to_jsonb(o)::text,'UTF8')),'hex'),
 jsonb_build_object('source','commercial_retention_owners','original_observed_at',o.observed_at,'original_source',o.source,
  'meaning','The original capture did not record its branch. This observation grants no recipient authority'),false
 FROM commercial_retention_owners o WHERE kind='invoice';

ALTER FUNCTION commercial_capture_retention_owner(text,text,uuid) RENAME TO commercial_capture_retention_owner_before_invoice_identity;
CREATE FUNCTION commercial_capture_retention_owner(p_number text,p_kind text,p_owner uuid DEFAULT NULL) RETURNS void
 LANGUAGE plpgsql AS $$
DECLARE owner_id uuid; owners uuid[]; basis text; current_evidence jsonb; observation jsonb; qualified boolean;
BEGIN
 IF p_kind<>'invoice' THEN
  PERFORM commercial_capture_retention_owner_before_invoice_identity(p_number,p_kind,p_owner); RETURN;
 END IF;
 SELECT user_id INTO owner_id FROM financial_documents WHERE number=p_number AND kind='invoice';
 IF owner_id IS NOT NULL THEN basis:='live_document_owner';
 ELSIF p_owner IS NOT NULL THEN owner_id:=p_owner; basis:='passed_document_owner';
 ELSE
  SELECT array_agg(DISTINCT subject) INTO owners FROM moderation_retained_record_owners WHERE kind='financial_document' AND record_id=p_number;
  IF cardinality(owners)=1 THEN owner_id:=owners[1]; basis:='exact_retained_document_owner'; END IF;
 END IF;
 current_evidence:=commercial_invoice_owner_evidence(p_number);
 IF owner_id IS NULL AND current_evidence->>'subject' IS NOT NULL THEN
  owner_id:=(current_evidence->>'subject')::uuid; basis:=current_evidence->>'basis';
 END IF;
 IF owner_id IS NULL THEN RETURN; END IF;
 -- A contradiction observed now never becomes a qualified witness later.
 qualified:=commercial_invoice_numeric(p_number) IS NOT NULL
  AND NOT commercial_invoice_identity_pending(p_number)
  AND (current_evidence IS NULL OR coalesce((current_evidence->>'subject')::uuid=owner_id AND (current_evidence->>'eligible')::boolean,false));
 observation:=jsonb_build_object('protocol',1,'number',p_number,'proposed_subject',owner_id,'basis',basis,
  'current_evidence',current_evidence,'identity_pending',commercial_invoice_identity_pending(p_number));
 INSERT INTO commercial_invoice_owner_observations(number,subject,basis,evidence_hash,evidence,qualified)
 VALUES(p_number,owner_id,basis,encode(sha256(convert_to(observation::text,'UTF8')),'hex'),observation,qualified) ON CONFLICT DO NOTHING;
 INSERT INTO commercial_retention_owners(number,kind,subject,source)
 VALUES(p_number,p_kind,owner_id,'observed_original_record_owner') ON CONFLICT DO NOTHING;
 IF current_evidence->>'basis' IN ('durable_numeric_payment','legacy_numeric_order')
  AND (current_evidence->>'subject')::uuid IS DISTINCT FROM owner_id THEN
  INSERT INTO commercial_invoice_identity_reviews(number,reason,source_key,evidence)
  VALUES(p_number,'numeric_recorded_owner_conflict',owner_id::text,observation) ON CONFLICT DO NOTHING;
 END IF;
END $$;

ALTER FUNCTION commercial_retention_owned(uuid,text,text) RENAME TO commercial_retention_owned_before_invoice_identity;
CREATE FUNCTION commercial_retention_owned(p_subject uuid,p_number text,p_kind text) RETURNS boolean
 LANGUAGE plpgsql AS $$
DECLARE current_evidence jsonb;
BEGIN
 IF p_kind<>'invoice' THEN RETURN commercial_retention_owned_before_invoice_identity(p_subject,p_number,p_kind); END IF;
 current_evidence:=commercial_invoice_owner_evidence(p_number);
 IF current_evidence IS NOT NULL THEN
  RETURN coalesce((current_evidence->>'subject')::uuid=p_subject AND (current_evidence->>'eligible')::boolean,false);
 END IF;
 RETURN coalesce((SELECT count(DISTINCT subject)=1 AND bool_and(subject=p_subject)
  FROM commercial_invoice_owner_observations WHERE number=p_number AND qualified),false);
END $$;

ALTER FUNCTION commercial_operation(text,uuid,jsonb) RENAME TO commercial_operation_before_invoice_identity;
CREATE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
DECLARE result jsonb;
BEGIN
 result:=commercial_operation_before_invoice_identity(p_operation,p_actor,p_body);
 IF p_operation='retention_queue' THEN
  RETURN result||jsonb_build_object(
   'invoice_identity_reviews',coalesce((SELECT jsonb_agg(q) FROM (SELECT * FROM commercial_invoice_identity_reviews ORDER BY observed_at,number LIMIT 100)q),'[]'),
   'unqualified_invoice_owner_observations',coalesce((SELECT jsonb_agg(q) FROM (SELECT * FROM commercial_invoice_owner_observations WHERE NOT qualified ORDER BY observed_at,number LIMIT 100)q),'[]'));
 END IF;
 RETURN result;
END $$;

-- Do not execute/materialize the old unsafe retained-record OR/PDF branch.
ALTER FUNCTION backend_moderation(text,uuid,jsonb) RENAME TO backend_moderation_before_invoice_identity;
CREATE FUNCTION backend_moderation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation='retained_records' THEN
  RETURN jsonb_build_object(
   'complete',EXISTS(SELECT 1 FROM users WHERE id=p_actor) OR EXISTS(SELECT 1 FROM moderation_erasure_events WHERE subject=p_actor AND retained_owner_inventory),
   'scope_note',CASE WHEN EXISTS(SELECT 1 FROM users WHERE id=p_actor) OR EXISTS(SELECT 1 FROM moderation_erasure_events WHERE subject=p_actor AND retained_owner_inventory)
    THEN 'Live owner references and preserved pre-erasure ownership identify surviving records. Erased account/profile fields are not reconstructed; archived documents remain available through their owned download routes.'
    ELSE 'Earlier erasure has no complete retained-owner inventory. Surviving directly owner-bound records are included; unlinked historical declarations/documents require proportionate human verification. Completeness is not established.' END,
   'paypal_payments',coalesce((SELECT jsonb_agg(to_jsonb(p)-'last_error') FROM paypal_payments p WHERE p.user_id=p_actor),'[]'::jsonb),
   'financial_documents',coalesce((SELECT jsonb_agg(to_jsonb(d)) FROM financial_documents d WHERE CASE WHEN d.kind='invoice' THEN commercial_invoice_original_owned(p_actor,d.number) AND NOT commercial_invoice_identity_pending(d.number) ELSE d.user_id=p_actor OR EXISTS(SELECT 1 FROM moderation_retained_record_owners o WHERE o.subject=p_actor AND o.kind='financial_document' AND o.record_id=d.number) END),'[]'::jsonb),
   'original_invoices',coalesce((SELECT jsonb_agg(jsonb_build_object('invoice_number',i.invoice_number,'pdf_base64',encode(i.pdf,'base64'),'provenance',i.provenance,'recorded_at',i.recorded_at)) FROM invoice_originals i WHERE commercial_invoice_original_owned(p_actor,i.invoice_number) AND NOT commercial_invoice_identity_pending(i.invoice_number)),'[]'::jsonb),
   'unavailable_invoice_references',coalesce((SELECT jsonb_agg(jsonb_build_object('number',q.number,'reason','identity_pending_review')) FROM (SELECT DISTINCT r.number FROM commercial_invoice_identity_reviews r WHERE commercial_invoice_original_owned(p_actor,r.number))q),'[]'::jsonb),
   'invoice_identity_scope','Known pending invoice contents are withheld without rewriting original evidence. Missing erased history cannot be retrospectively verified.',
   'contract_declarations',coalesce((SELECT jsonb_agg(to_jsonb(d)) FROM contract_declarations d WHERE d.user_id=p_actor OR EXISTS(SELECT 1 FROM moderation_retained_record_owners o WHERE o.subject=p_actor AND o.kind='contract_declaration' AND o.record_id=d.id::text)),'[]'::jsonb),
   'paypal_legacy_reconciliation',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM paypal_legacy_reconciliation r WHERE r.user_id=p_actor),'[]'::jsonb),
   'premium_period_changes',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM premium_period_changes r WHERE r.user_id=p_actor),'[]'::jsonb),
   'internal_coin_operations',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM internal_coin_operations r WHERE r.user_id=p_actor),'[]'::jsonb),
   'contract_evidence',coalesce((SELECT jsonb_agg(jsonb_build_object('declaration',to_jsonb(d),'contract_delivery',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_delivery r WHERE r.declaration_id=d.id),'[]'::jsonb),'contract_cancellation_schedule',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_cancellation_schedule r WHERE r.declaration_id=d.id),'[]'::jsonb),'contract_account_observation',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_account_observation r WHERE r.declaration_id=d.id),'[]'::jsonb),'contract_period_observation',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_period_observation r WHERE r.declaration_id=d.id),'[]'::jsonb),'contract_premium_operations',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_premium_operations r WHERE r.declaration_id=d.id),'[]'::jsonb),'contract_processing_actions',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_processing_actions r WHERE r.declaration_id=d.id),'[]'::jsonb),'contract_purchase_observations',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM contract_purchase_observations r WHERE r.declaration_id=d.id),'[]'::jsonb))) FROM contract_declarations d WHERE d.user_id=p_actor OR EXISTS(SELECT 1 FROM moderation_retained_record_owners o WHERE o.subject=p_actor AND o.kind='contract_declaration' AND o.record_id=d.id::text) OR EXISTS(SELECT 1 FROM contract_account_observation a WHERE a.declaration_id=d.id AND a.user_id=p_actor)),'[]'::jsonb),
   'paypal_receipt_artifacts',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM paypal_receipt_artifacts r WHERE EXISTS(SELECT 1 FROM paypal_payments p WHERE p.order_id=r.order_id AND p.user_id=p_actor)),'[]'::jsonb),
   'paypal_receipt_observations',coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM paypal_receipt_observations r WHERE EXISTS(SELECT 1 FROM paypal_payments p WHERE p.order_id=r.order_id AND p.user_id=p_actor)),'[]'::jsonb),
   'withdrawal_consents',coalesce((SELECT jsonb_agg(to_jsonb(w)) FROM withdrawal_consents w WHERE w.user_id=p_actor),'[]'::jsonb));
 END IF;
 RETURN backend_moderation_before_invoice_identity(p_operation,p_actor,p_body);
END $$;
