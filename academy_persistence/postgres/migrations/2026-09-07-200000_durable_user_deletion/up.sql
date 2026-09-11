-- Work is deliberately independent of the erased account and financial evidence.
create table user_deletion_work (
    user_id uuid not null,
    service text not null check (service in ('skills', 'challenges', 'events')),
    requested_at timestamptz not null default now(),
    next_attempt_at timestamptz not null default now(),
    attempts bigint not null default 0,
    last_error text,
    primary key (user_id, service)
);
create index user_deletion_work_due on user_deletion_work (next_attempt_at, user_id, service);
create function queue_user_deletion() returns trigger language plpgsql as $$
begin
    insert into user_deletion_work (user_id, service)
      select old.id, service from unnest(array['skills','challenges','events']) service
      on conflict do nothing;
    return old;
end;
$$;
create trigger queue_user_deletion before delete on users
    for each row execute function queue_user_deletion();
