CREATE TABLE users (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT,
    email_verified BOOLEAN NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    last_login TIMESTAMP WITH TIME ZONE,
    last_name_change TIMESTAMP WITH TIME ZONE,
    enabled BOOLEAN NOT NULL,
    admin BOOLEAN NOT NULL,
    newsletter BOOLEAN NOT NULL
);
CREATE UNIQUE INDEX users_name_idx ON users (lower(name));
CREATE UNIQUE INDEX users_email_idx ON users (lower(email));

CREATE TABLE user_profiles (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL,
    bio TEXT NOT NULL,
    tags TEXT[] NOT NULL
);
ALTER TABLE users ADD FOREIGN KEY (id) REFERENCES user_profiles(user_id) INITIALLY DEFERRED;

CREATE TABLE user_passwords (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    password_hash TEXT NOT NULL
);
