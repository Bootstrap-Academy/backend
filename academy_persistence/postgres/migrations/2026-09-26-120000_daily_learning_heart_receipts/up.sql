-- Add a zero-charge receipt for a new attempt under the daily lesson policy.
-- Existing financial/history records are unchanged.
ALTER TABLE internal_heart_operations
    DROP CONSTRAINT internal_heart_operations_outcome_check,
    DROP CONSTRAINT internal_heart_operations_check,
    ADD CONSTRAINT internal_heart_operations_outcome_check
        CHECK (outcome IN ('charged', 'premium', 'insufficient', 'daily_learning')),
    ADD CONSTRAINT internal_heart_operations_check
        CHECK ((outcome = 'charged' AND charged_half_hearts = 2)
            OR (outcome IN ('premium', 'insufficient', 'daily_learning') AND charged_half_hearts = 0));
