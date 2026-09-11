-- Pending evidence and already published preservation cannot be downgraded away.
DO $$ BEGIN
 RAISE EXCEPTION 'Invoice disposal preservation is forward-repair-only; protection and history remain installed';
END $$;
