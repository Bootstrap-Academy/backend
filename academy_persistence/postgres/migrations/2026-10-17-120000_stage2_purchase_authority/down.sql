-- The authorization of stage 1 (2026-09-08-210000_learning_purchase_authority). Offers of
-- stage 2 that wait for an authorization cannot be accepted on this path afterwards; nothing
-- already authorized or accepted changes.
CREATE OR REPLACE FUNCTION commercial_purchase_authorize(p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
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
