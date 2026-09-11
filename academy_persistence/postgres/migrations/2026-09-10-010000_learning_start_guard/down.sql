-- Refuse a silent downgrade that could follow an unintended active subject.
DO $$ BEGIN RAISE EXCEPTION 'Learning creation guard requires an explicit reviewed forward correction'; END $$;
