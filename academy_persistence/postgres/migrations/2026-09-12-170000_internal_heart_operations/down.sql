DO $$ BEGIN
    RAISE EXCEPTION 'Heart operation replay receipts must not be removed by a downgrade';
END $$;
