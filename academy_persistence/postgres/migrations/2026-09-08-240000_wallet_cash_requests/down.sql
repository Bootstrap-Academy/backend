DO $$ BEGIN RAISE EXCEPTION 'Existing wallet-value claims and original dispositions require a reviewed forward migration'; END $$;
