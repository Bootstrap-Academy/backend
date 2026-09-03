alter table users
  add column terms_declined_at timestamp with time zone;

drop view if exists user_composites;
create view user_composites as (
  select * from users u
    inner join user_profiles p on u.id=p.user_id
    inner join user_details d using (user_id)
    inner join user_invoice_info i using (user_id)
);
