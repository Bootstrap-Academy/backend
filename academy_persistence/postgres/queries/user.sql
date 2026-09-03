--: UserComposite (email?, last_login?, last_name_change?, terms_version?, terms_accepted_at?, age_confirmed_at?, business?, first_name?, last_name?, street?, zip_code?, city?, country?, vat_id?)

--! count_composites (name?, email?, enabled?, admin?, mfa_enabled?, email_verified?)
select count(*) from user_composites
  where (:name::text is null
    or position(lower(:name) in lower(name)) > 0
    or position(lower(:name) in lower(display_name)) > 0)
  and (:email::text is null or position(lower(:email) in email) > 0)
  and (:enabled::boolean is null or enabled = :enabled)
  and (:admin::boolean is null or admin = :admin)
  and (:mfa_enabled::boolean is null or mfa_enabled = :mfa_enabled)
  and (:email_verified::boolean is null or email_verified = :email_verified);

--! list_composites (name?, email?, enabled?, admin?, mfa_enabled?, email_verified?) : UserComposite
select * from user_composites
  where (:name::text is null
    or position(lower(:name) in lower(name)) > 0
    or position(lower(:name) in lower(display_name)) > 0)
  and (:email::text is null or position(lower(:email) in email) > 0)
  and (:enabled::boolean is null or enabled = :enabled)
  and (:admin::boolean is null or admin = :admin)
  and (:mfa_enabled::boolean is null or mfa_enabled = :mfa_enabled)
  and (:email_verified::boolean is null or email_verified = :email_verified)
  order by created_at asc
  limit :limit offset :offset;

--! exists
select (exists (select 1 from users where id=:id));

--! get_composite : UserComposite
select * from user_composites where id=:id;

--! get_composite_by_name : UserComposite
select * from user_composites where lower(name)=lower(:name);

--! get_composite_by_email : UserComposite
select * from user_composites where lower(email)=lower(:email);

--! get_composite_by_oauth2_provider_id_and_remote_user_id : UserComposite
with cte as (
  select user_id as id from oauth2_links where provider_id=:provider_id and remote_user_id=:remote_user_id
)
select * from user_composites inner join cte using (id);

--! create (email?, last_login?, last_name_change?, terms_version?, terms_accepted_at?, age_confirmed_at?)
insert into users (id, name, email, email_verified, created_at, last_login, last_name_change, enabled, admin, terms_version, terms_accepted_at, age_confirmed_at)
  values (:id, :name, :email, :email_verified, :created_at, :last_login, :last_name_change, :enabled, :admin, :terms_version, :terms_accepted_at, :age_confirmed_at);

--! create_profile
insert into user_profiles (user_id, display_name, bio, tags)
  values (:user_id, :display_name, :bio, :tags);

--! create_invoice_info (business?, first_name?, last_name?, street?, zip_code?, city?, country?, vat_id?)
insert into user_invoice_info (user_id, business, first_name, last_name, street, zip_code, city, country, vat_id)
  values (:user_id, :business, :first_name, :last_name, :street, :zip_code, :city, :country, :vat_id);

--! update (name?, email?, email_verified?, last_login?, last_name_change?, enabled?, admin?)
update users
  set
    name=coalesce(:name, name),
    email=coalesce(:email, email),
    email_verified=coalesce(:email_verified, email_verified),
    last_login=coalesce(:last_login, last_login),
    last_name_change=coalesce(:last_name_change, last_name_change),
    enabled=coalesce(:enabled, enabled),
    admin=coalesce(:admin, admin)
  where id=:id;

--! update_profile (display_name?, bio?, tags?)
update user_profiles
  set
    display_name=coalesce(:display_name, display_name),
    bio=coalesce(:bio, bio),
    tags=coalesce(:tags, tags)
  where user_id=:user_id;

--! update_invoice_info (business?, first_name?, last_name?, street?, zip_code?, city?, country?, vat_id?)
update user_invoice_info
  set
    business=case when :clear_business then null else coalesce(:business, business) end,
    first_name=case when :clear_first_name then null else coalesce(:first_name, first_name) end,
    last_name=case when :clear_last_name then null else coalesce(:last_name, last_name) end,
    street=case when :clear_street then null else coalesce(:street, street) end,
    zip_code=case when :clear_zip_code then null else coalesce(:zip_code, zip_code) end,
    city=case when :clear_city then null else coalesce(:city, city) end,
    country=case when :clear_country then null else coalesce(:country, country) end,
    vat_id=case when :clear_vat_id then null else coalesce(:vat_id, vat_id) end
  where user_id=:user_id;

--! update_terms_acceptance
update users
  set
    terms_version=:terms_version,
    terms_accepted_at=:terms_accepted_at,
    age_confirmed_at=:age_confirmed_at
  where id=:id;

--! delete
delete from users where id=:id;

--! get_password_hash
select password_hash from user_passwords where user_id=:user_id;

--! set_password_hash
insert into user_passwords (user_id, password_hash)
  values (:user_id, :password_hash)
  on conflict (user_id) do update set password_hash=:password_hash;

--! remove_password_hash
delete from user_passwords where user_id=:user_id;

--! get_number
merge into user_numbers
  using (select :user_id::uuid as user_id) s
  on user_numbers.user_id = s.user_id
  when matched then update set number=number
  when not matched then insert (user_id, number) values (:user_id, nextval('user_number'))
  returning number;
