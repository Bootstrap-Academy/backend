create table oauth2_links (
    id uuid primary key,
    user_id uuid not null references users(id) on delete cascade,
    provider_id text not null,
    created_at timestamp with time zone not null,
    remote_user_id text not null,
    remote_user_name text not null
);

create unique index oauth2_links_provider_id_remote_user_id_idx on oauth2_links (provider_id, remote_user_id);
