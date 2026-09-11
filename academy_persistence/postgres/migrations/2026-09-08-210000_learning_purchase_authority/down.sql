DO $$ BEGIN RAISE EXCEPTION 'Retained claimant elections require an explicit reviewed forward migration'; END $$;
