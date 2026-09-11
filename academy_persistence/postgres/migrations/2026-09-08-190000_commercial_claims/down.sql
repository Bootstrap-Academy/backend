-- Deleting this authority can make already reserved/paid value spendable again or
-- discard original requests. Rollback needs a reviewed compatible preservation plan.
DO $$ BEGIN RAISE EXCEPTION 'Commercial evidence requires a reviewed preservation migration; automatic rollback refused'; END $$;
