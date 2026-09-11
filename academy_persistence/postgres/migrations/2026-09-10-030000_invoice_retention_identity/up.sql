-- IF1-R1/R2: preserve original identities and immutable observations while
-- withholding rich invoice retention evidence whose identity needs review.
CREATE OR REPLACE FUNCTION commercial_retention_owned(p_subject uuid,p_number text,p_kind text) RETURNS boolean
 LANGUAGE plpgsql AS $$
DECLARE current_evidence jsonb;
BEGIN
 IF p_kind<>'invoice' THEN RETURN commercial_retention_owned_before_invoice_identity(p_subject,p_number,p_kind); END IF;
 -- This applies to both a current numeric owner and a qualified past witness.
 -- The separate original-owner predicate still admits its minimal unavailable
 -- reference; identity review must never expose ambiguous rich content.
 IF commercial_invoice_identity_pending(p_number) THEN RETURN false; END IF;
 current_evidence:=commercial_invoice_owner_evidence(p_number);
 IF current_evidence IS NOT NULL THEN
  RETURN coalesce((current_evidence->>'subject')::uuid=p_subject AND (current_evidence->>'eligible')::boolean,false);
 END IF;
 RETURN coalesce((SELECT count(DISTINCT subject)=1 AND bool_and(subject=p_subject)
  FROM commercial_invoice_owner_observations WHERE number=p_number AND qualified),false);
END $$;

-- Observe the effective document owner now: a live owner takes precedence;
-- otherwise the exact durable pre-erasure inventory is the witness. Its key
-- (kind,record_id) is unique. Generic commercial associations are not proof.
-- Do not change numeric original ownership, old rows, or earlier observations.
INSERT INTO commercial_invoice_identity_reviews(number,reason,source_key,evidence)
 SELECT d.number,'numeric_recorded_owner_conflict',d.number,
 jsonb_build_object(
  'source','invoice_retention_identity_forward',
  'recorded_subject',coalesce(d.user_id,o.subject),
  'recorded_owner_basis',CASE WHEN d.user_id IS NOT NULL THEN 'live_document_owner' ELSE 'exact_retained_document_owner' END,
  'retained_witness',CASE WHEN d.user_id IS NULL THEN to_jsonb(o) END,
  'retained_witness_sha256',CASE WHEN d.user_id IS NULL THEN encode(sha256(convert_to(to_jsonb(o)::text,'UTF8')),'hex') END,
  'current_numeric_evidence',e,
  'document_sha256',encode(sha256(convert_to(to_jsonb(d)::text,'UTF8')),'hex'))
 FROM financial_documents d
 LEFT JOIN moderation_retained_record_owners o ON o.kind='financial_document' AND o.record_id=d.number
 CROSS JOIN LATERAL (SELECT commercial_invoice_owner_evidence(d.number) e) q
 WHERE d.kind='invoice' AND coalesce(d.user_id,o.subject) IS NOT NULL
 AND e->>'basis' IN ('durable_numeric_payment','legacy_numeric_order')
 AND (e->>'subject')::uuid IS DISTINCT FROM coalesce(d.user_id,o.subject)
 ON CONFLICT(number,reason,source_key) DO NOTHING;
