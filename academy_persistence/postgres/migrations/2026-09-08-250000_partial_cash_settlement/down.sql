DO $$ BEGIN RAISE EXCEPTION 'Original partitions, payment allocations and surviving claims require a reviewed forward migration'; END $$;
