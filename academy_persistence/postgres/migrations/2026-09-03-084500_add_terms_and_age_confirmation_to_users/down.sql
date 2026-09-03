drop view if exists user_composites;

alter table users
  drop column terms_version,
  drop column terms_accepted_at,
  drop column age_confirmed_at;

create view user_composites as (
  select * from users u
    inner join user_profiles p on u.id=p.user_id
    inner join user_details d using (user_id)
    inner join user_invoice_info i using (user_id)
);
