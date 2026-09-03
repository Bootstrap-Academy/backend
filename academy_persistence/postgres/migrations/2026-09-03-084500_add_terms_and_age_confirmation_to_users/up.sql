alter table users
  add column terms_version text,
  add column terms_accepted_at timestamp with time zone,
  add column age_confirmed_at timestamp with time zone;

drop view if exists user_composites;
create view user_composites as (
  select * from users u
    inner join user_profiles p on u.id=p.user_id
    inner join user_details d using (user_id)
    inner join user_invoice_info i using (user_id)
);
