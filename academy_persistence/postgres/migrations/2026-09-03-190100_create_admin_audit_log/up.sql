-- Append-only record of the state changing requests made with an
-- administrator's access token. Request bodies are never stored.
--
-- Neither user id references the users table: the log has to stay complete and
-- attributable even after the acting or the affected account is deleted. Rows
-- are removed by `academy task prune-database` twelve months after the request.
create table admin_audit_log (
    id uuid primary key,
    at timestamp with time zone not null,
    admin_user_id uuid not null,
    method text not null,
    path text not null,
    target_user_id uuid,
    status integer not null,
    request_id text not null
);

create index admin_audit_log_at_idx on admin_audit_log (at desc);
create index admin_audit_log_admin_user_id_idx on admin_audit_log (admin_user_id);
create index admin_audit_log_target_user_id_idx on admin_audit_log (target_user_id);
