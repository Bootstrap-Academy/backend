CREATE TABLE sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_name TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL
);

CREATE TABLE session_refresh_tokens (
    session_id UUID PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    refresh_token_hash BYTEA NOT NULL
);
