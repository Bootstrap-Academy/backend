--: Session (device_name?)

--! get : Session
select * from sessions where id=:id;

--! get_by_refresh_token_hash : Session
select s.* from sessions s
  inner join session_refresh_tokens rt
  on s.id=rt.session_id
  where rt.refresh_token_hash=:refresh_token_hash;

--! list_by_user : Session
select * from sessions where user_id=:user_id;

--! create (device_name?)
insert into sessions (id, user_id, device_name, created_at, updated_at)
  values (:id, :user_id, :device_name, :created_at, :updated_at);

--! update (device_name?, updated_at?)
update sessions
  set
    device_name=case when :clear_device_name then null else coalesce(:device_name, device_name) end,
    updated_at=coalesce(:updated_at, updated_at)
  where id=:id;

--! delete
delete from sessions where id=:id;

--! delete_by_user
delete from sessions where user_id=:user_id;

--! delete_by_updated_at
delete from sessions where updated_at<:updated_at;

--! list_refresh_token_hashes_by_user
select rt.refresh_token_hash
  from session_refresh_tokens rt
  inner join sessions s on s.id=rt.session_id
  where s.user_id=:user_id;

--! get_refresh_token_hash
select refresh_token_hash from session_refresh_tokens where session_id=:session_id;

--! set_refresh_token_hash
insert into session_refresh_tokens (session_id, refresh_token_hash)
  values (:session_id, :refresh_token_hash)
  on conflict (session_id) do update set refresh_token_hash=:refresh_token_hash;
