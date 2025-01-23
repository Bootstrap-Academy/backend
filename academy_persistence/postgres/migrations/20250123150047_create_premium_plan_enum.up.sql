create type premium_plan as enum ('monthly', 'yearly');

alter table premium_subscriptions alter column plan type premium_plan using
  case when plan=0 then 'monthly'::premium_plan when plan=1 then 'yearly' end;
