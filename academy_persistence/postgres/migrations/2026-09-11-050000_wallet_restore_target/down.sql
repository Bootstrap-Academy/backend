-- Removing this guard could redirect a prepared restoration to a replacement subject.
DO $$ BEGIN
 RAISE EXCEPTION 'Wallet restoration target and isolation protection requires forward repair; unsafe downgrade refused';
END $$;
