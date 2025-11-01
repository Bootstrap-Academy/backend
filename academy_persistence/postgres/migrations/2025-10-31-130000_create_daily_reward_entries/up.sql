create type daily_reward_category as enum ('arrival', 'lecture', 'practice', 'lab');

create table daily_reward_entries (
    id uuid primary key,
    user_id uuid not null references users(id) on delete cascade,
    date_utc date not null,
    category daily_reward_category not null,
    coins integer not null check (coins >= 0),
    first_detected_at timestamptz,
    last_detected_at timestamptz,
    claimable_since timestamptz,
    claimed_at timestamptz,
    activity_sample jsonb,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint chk_claimable_since check (claimable_since is null or first_detected_at is not null),
    constraint chk_claimed_at check (claimed_at is null or claimable_since is not null)
);

create unique index daily_reward_entries_user_date_category_idx on daily_reward_entries (user_id, date_utc, category);
create index daily_reward_entries_claimable_idx on daily_reward_entries (user_id, date_utc) where claimable_since is not null and claimed_at is null;

