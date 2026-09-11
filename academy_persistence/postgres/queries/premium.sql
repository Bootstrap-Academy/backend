--: Premium()

--! get_latest_by_user_id : Premium
select * from premium where user_id=:user_id order by until desc limit 1;

--! create
insert into premium (id, user_id, since, until)
  values (:id, :user_id, :since, :until);

--! extend
update premium set until=:until where id=:id;

--! list_subscription_users
select user_id from premium_subscriptions;

--! get_subscription
select plan from premium_subscriptions where user_id=:user_id;

--! set_subscription
insert into premium_subscriptions (user_id, plan) values (:user_id, :plan)
  on conflict (user_id) do update set plan=excluded.plan;

--! delete_subscription
delete from premium_subscriptions where user_id=:user_id;
