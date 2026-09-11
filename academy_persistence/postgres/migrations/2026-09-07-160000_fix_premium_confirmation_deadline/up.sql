-- A later paid purchase must never erase a missed confirmation deadline.
-- Keep the activation snapshot on the immutable agreement, independent of
-- premium.until and subsequent paid-period rows. No FK: retained evidence must
-- survive deletion of paid access/account data.
alter table premium_renewal_agreements
    add column paid_period_id uuid,
    add column confirmation_deadline timestamptz,
    add constraint premium_renewal_activation_snapshot
        check ((paid_period_id is null) = (confirmation_deadline is null));

-- Existing records contain no reliable deadline or successful-send time.
-- Do not invent either from today's mutable premium rows. Preserve evidence
-- and paid access, and require a fresh explicit order before further renewal.
insert into premium_renewal_cancellations (agreement_id, paid_until)
select s.agreement_id, (select max(p.until) from premium p where p.user_id=s.user_id)
from premium_subscriptions s where s.agreement_id is not null
on conflict do nothing;
delete from premium_subscriptions where agreement_id is not null;
