alter table premium_subscriptions alter column plan type smallint using
  case when plan='monthly' then 0 when plan='yearly' then 1 end;

drop type premium_plan;
