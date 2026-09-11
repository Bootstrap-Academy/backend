DO $$ BEGIN RAISE EXCEPTION 'Moderation evidence requires a reviewed preservation migration before rollback'; END $$;
