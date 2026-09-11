-- Never restore archived automatic debits during rollback. The old code cannot
-- enforce the new consent/price rules, so leave all renewal OFF on downgrade.
do $$ begin
    if exists (select 1 from premium_legacy_renewals) or exists (select 1 from premium_renewal_agreements) then
        raise exception 'Cannot downgrade with retained renewal evidence; use a forward corrective migration';
    end if;
end $$;
delete from premium_subscriptions;
drop trigger end_premium_renewal_on_user_deletion on users;
drop function end_premium_renewal_on_user_deletion();
alter table premium_subscriptions drop column agreement_id;
drop table premium_renewal_cancellations;
drop table premium_renewal_delivery;
drop table premium_renewal_agreements;
drop table premium_legacy_renewals;
drop function preserve_premium_renewal_evidence();
