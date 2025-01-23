--: OAuth2Link()

--! list_links_by_user : OAuth2Link
select * from oauth2_links where user_id=:user_id;

--! get_link : OAuth2Link
select * from oauth2_links where id=:id;

--! create_link
insert into oauth2_links (id, user_id, provider_id, created_at, remote_user_id, remote_user_name)
  values (:id, :user_id, :provider_id, :created_at, :remote_user_id, :remote_user_name);

--! delete_link
delete from oauth2_links where id=:id;
