DO $$ BEGIN RAISE EXCEPTION 'Retained service identity and contract evidence require an assessed migration; automatic destructive rollback is unavailable'; END $$;
