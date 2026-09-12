-- Ordinary private learning receipts; account deletion removes them with the user.
-- No historical attempt is charged or imported by this migration.
CREATE TABLE internal_heart_operations (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    half_hearts bigint NOT NULL CHECK (half_hearts = 2),
    reason text NOT NULL CHECK (reason = 'incorrect_challenge_attempt'),
    charged_half_hearts bigint NOT NULL CHECK (charged_half_hearts IN (0, 2)),
    hearts bigint NOT NULL CHECK (hearts >= 0),
    outcome text NOT NULL CHECK (outcome IN ('charged', 'premium', 'insufficient')),
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((outcome = 'charged' AND charged_half_hearts = 2)
        OR (outcome IN ('premium', 'insufficient') AND charged_half_hearts = 0))
);
CREATE INDEX internal_heart_operations_user_id ON internal_heart_operations (user_id);
