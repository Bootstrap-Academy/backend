-- Original evidence and pending qualification cannot be discarded by downgrade.
DO $$ BEGIN RAISE EXCEPTION 'Invoice identity correction requires an explicit reviewed forward migration'; END $$;
