DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM purchase_provision_observations) OR EXISTS(SELECT 1 FROM purchase_offers o JOIN purchase_acceptances a ON a.order_id=o.id WHERE o.offer ? 'provision_window_seconds') THEN
  RAISE EXCEPTION 'Cannot discard accepted deadline or committed provision evidence';
 END IF;
END $$;
DROP TABLE purchase_provision_observations;
DROP FUNCTION purchase_provision_deadline(uuid);
