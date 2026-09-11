DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM premium_period_commit_observation) OR EXISTS(SELECT 1 FROM contract_premium_operations WHERE evidence ? 'commit_observed_at') THEN
   RAISE EXCEPTION 'Retained committed-period observations require forward repair';
 END IF;
END $$;
DROP TABLE premium_period_commit_observation;
DROP FUNCTION observe_committed_premium_period();
