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

--! set_subscription (plan?)
merge into premium_subscriptions
  using (select :user_id::uuid as user_id where :plan::premium_plan is not null) as s
  on premium_subscriptions.user_id = s.user_id
  when not matched by target then insert (user_id, plan) values (:user_id, :plan)
  when not matched by source then delete
  when matched then update set plan=:plan;
