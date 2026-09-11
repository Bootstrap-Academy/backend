-- Review-2: merits closure does not complete an independently assessed retention review.
-- Recompute after the closure UPDATE (including any row-lock wait), from the
-- surviving authoritative work records. This does not add retention or a review event.
CREATE FUNCTION moderation_closure_review_due() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM moderation_update_review_due(NEW.id);
 RETURN NEW;
END $$;
CREATE TRIGGER moderation_closure_review_due AFTER UPDATE OF closed_at ON moderation_cases
 FOR EACH ROW WHEN (OLD.closed_at IS DISTINCT FROM NEW.closed_at)
 EXECUTE FUNCTION moderation_closure_review_due();

-- Due work remains ahead of later work regardless of whether the merits case
-- has closed. Completed cases with no obligation remain available at the end.
CREATE OR REPLACE FUNCTION moderation_queue(p_limit integer,p_offset integer) RETURNS jsonb LANGUAGE sql VOLATILE AS $$
 SELECT coalesce(jsonb_agg(to_jsonb(c)||jsonb_build_object(
 'decisions',coalesce((SELECT jsonb_agg(to_jsonb(d) ORDER BY d.created_at) FROM moderation_decisions d WHERE d.case_id=c.id),'[]'),
 'complaints',coalesce((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.received_at) FROM moderation_complaints a WHERE a.case_id=c.id),'[]'),
 'escalations',coalesce((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.recorded_at) FROM moderation_escalations e WHERE e.case_id=c.id),'[]'),
 'effective',moderation_effect(c.target_kind,c.target_id))), '[]')
 FROM (SELECT * FROM moderation_cases ORDER BY review_due_at NULLS LAST,closed_at NULLS FIRST,received_at,id LIMIT least(greatest(p_limit,1),100) OFFSET greatest(p_offset,0)) c
$$;

-- Repair the missing materialized date of already-closed cases from their
-- existing obligations; immutable assessment/notice/decision history is untouched.
SELECT moderation_update_review_due(id) FROM moderation_cases WHERE closed_at IS NOT NULL;
