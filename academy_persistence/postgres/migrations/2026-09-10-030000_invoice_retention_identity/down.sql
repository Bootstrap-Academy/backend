-- Neither a downgrade nor a migration rollback may discard review evidence
-- or reopen rich content whose original identity remains unresolved.
DO $$ BEGIN RAISE EXCEPTION 'Invoice retention identity correction requires an explicit reviewed forward migration'; END $$;
