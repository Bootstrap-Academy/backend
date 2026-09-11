DO $$ BEGIN RAISE EXCEPTION 'Pending retention work must remain scheduled; use a reviewed preservation migration'; END $$;
