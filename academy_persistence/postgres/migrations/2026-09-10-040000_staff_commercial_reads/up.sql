-- Read-only staff capacity observations. This does not reserve or qualify value.
CREATE FUNCTION commercial_admin_cash_capacity(p_case uuid,p_subject uuid) RETURNS jsonb
LANGUAGE sql STABLE AS $$
 WITH selected AS (
  SELECT c.id,c.subject,EXISTS(SELECT 1 FROM users u WHERE u.id=c.subject) AS live
  FROM commercial_cases c WHERE c.id=p_case AND c.subject=p_subject
 ), capture AS (
  SELECT c.*,CASE WHEN c.live THEN coalesce((SELECT sum(o.coins) FROM paypal_coin_orders o
    WHERE o.user_id=c.subject AND o.captured_at IS NOT NULL),0)
   ELSE (e.evidence->>'captured_purchase_units')::bigint END AS captured,
   CASE WHEN c.live THEN (SELECT count(*) FROM paypal_coin_orders o
    WHERE o.user_id=c.subject AND o.captured_at IS NOT NULL) END AS live_count,
   e.id AS evidence_id,e.recorded_at AS evidence_recorded_at
  FROM selected c LEFT JOIN commercial_evidence e ON NOT c.live AND e.case_id=c.id
   AND e.category='wallet_boundary' AND e.source_key=c.subject::text
 ), owned AS (
  SELECT o.* FROM commercial_obligations o JOIN selected c ON c.id=o.case_id
 ), reservations AS (
  SELECT r.* FROM commercial_reservations r JOIN owned o ON o.id=r.obligation_id
 ), counted AS (
  SELECT * FROM reservations WHERE state IN ('reserved','uncertain','completed')
 ), cash AS (
  SELECT coalesce(sum(purchase_capacity_units) FILTER(WHERE state='reserved'),0) AS reserved,
   coalesce(sum(purchase_capacity_units) FILTER(WHERE state='uncertain'),0) AS uncertain,
   coalesce(sum(purchase_capacity_units) FILTER(WHERE state='completed'),0) AS completed,
   count(*) FILTER(WHERE purchase_capacity_units IS NULL) AS unknowns,
   coalesce(sum(purchase_capacity_units),0) AS total
  FROM counted WHERE mode='cash'
 ), obligation_totals AS (
  SELECT o.*,coalesce((SELECT sum(r.units) FROM counted r WHERE r.obligation_id=o.id),0) AS consumed,
   coalesce((SELECT sum(r.units) FROM counted r WHERE r.obligation_id=o.id AND r.mode='cash'),0) AS cash_consumed
  FROM owned o
 )
 SELECT jsonb_build_object(
  'protocol',1,'case_id',c.id,'subject',c.subject,'observed_at',statement_timestamp(),
  'currency','EUR','units_per_eur',100,'coin_policy','fungible_reward_first',
  'captured_purchase_units',c.captured::text,
  'captured_basis',jsonb_build_object(
   'kind',CASE WHEN c.live THEN 'live_original_claimant_capture_records' ELSE 'preserved_original_claimant_erasure_observation' END,
   'live_captured_record_count',c.live_count::text,'evidence_id',c.evidence_id,'recorded_at',c.evidence_recorded_at),
  'historic_prior_refund_status',CASE WHEN b.case_id IS NULL THEN 'unknown' ELSE 'operator_reviewed' END,
  'historic_prior_refund_units',b.prior_refund_units::text,
  'historic_prior_refund_review',CASE WHEN b.case_id IS NULL THEN NULL ELSE jsonb_build_object(
   'case_id',b.case_id,'prior_refund_units',b.prior_refund_units::text,'assessment',b.assessment,'reviewed_at',b.reviewed_at) END,
  'known_reserved_purchase_capacity_units',cash.reserved::text,
  'known_uncertain_purchase_capacity_units',cash.uncertain::text,
  'known_recorded_completed_purchase_capacity_units',cash.completed::text,
  'unknown_reservation_capacity_count',cash.unknowns::text,
  'remaining_purchase_capacity',CASE WHEN b.case_id IS NULL OR c.captured IS NULL OR cash.unknowns>0 THEN NULL
   ELSE greatest(0,c.captured-b.prior_refund_units-cash.total)::text END,
  'obligations',coalesce((SELECT jsonb_agg(jsonb_build_object(
   'id',o.id,'source',o.source,'source_key',o.source_key,'component',o.component,'status',o.status,
   'units',o.units::text,'cash_units',o.cash_units::text,
   'counted_reservation_units',o.consumed::text,'counted_cash_reservation_units',o.cash_consumed::text,
   'remaining_units',CASE WHEN o.units IS NULL THEN NULL
    WHEN o.status IN ('historical_wallet_application','rejected') THEN '0' ELSE greatest(0,o.units-o.consumed)::text END,
   'remaining_cash_units',CASE WHEN o.cash_units IS NULL THEN NULL ELSE greatest(0,o.cash_units-o.cash_consumed)::text END
  ) ORDER BY o.id) FROM obligation_totals o),'[]'::jsonb),
  'reservations',coalesce((SELECT jsonb_agg(jsonb_build_object(
   'id',r.id,'obligation_id',r.obligation_id,'parent_id',r.parent_id,'mode',r.mode,'state',r.state,
   'units',r.units::text,'purchase_capacity_units',r.purchase_capacity_units::text
  ) ORDER BY r.id) FROM reservations r),'[]'::jsonb)
 ) FROM capture c CROSS JOIN cash LEFT JOIN commercial_cash_basis b ON b.case_id=c.id;
$$;

-- Preserve the effective invoice-identity wrapper and every older operation.
ALTER FUNCTION commercial_operation(text,uuid,jsonb) RENAME TO commercial_operation_before_staff_reads;
CREATE FUNCTION commercial_operation(p_operation text,p_actor uuid,p_body jsonb) RETURNS jsonb LANGUAGE plpgsql AS $$
BEGIN
 IF p_operation='admin_cash_capacity' THEN
  IF jsonb_typeof(p_body) IS DISTINCT FROM 'object'
   OR NOT (p_body ?& ARRAY['case_id','subject','_staff_session','_staff_refresh_hash'])
   OR p_body-ARRAY['case_id','subject','_staff_session','_staff_refresh_hash']<>'{}'::jsonb
   OR jsonb_typeof(p_body->'case_id') IS DISTINCT FROM 'string'
   OR jsonb_typeof(p_body->'subject') IS DISTINCT FROM 'string'
   OR jsonb_typeof(p_body->'_staff_session') IS DISTINCT FROM 'string'
   OR jsonb_typeof(p_body->'_staff_refresh_hash') IS DISTINCT FROM 'string' THEN
   RAISE EXCEPTION 'Exact staff capacity read fields required';
  END IF;
  IF NOT commercial_staff_active(p_actor,p_body) THEN RAISE EXCEPTION 'Current administrator MFA authority required'; END IF;
  RETURN commercial_admin_cash_capacity((p_body->>'case_id')::uuid,(p_body->>'subject')::uuid);
 END IF;
 RETURN commercial_operation_before_staff_reads(p_operation,p_actor,p_body);
END $$;
