DO $$ BEGIN
 RAISE EXCEPTION 'Historical moderation email protection requires a reviewed forward migration';
END $$;
