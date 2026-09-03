alter table user_profiles
  add column leaderboard_opt_out boolean not null default false;

drop view if exists user_composites;
create view user_composites as (
  select * from users u
    inner join user_profiles p on u.id=p.user_id
    inner join user_details d using (user_id)
    inner join user_invoice_info i using (user_id)
);
