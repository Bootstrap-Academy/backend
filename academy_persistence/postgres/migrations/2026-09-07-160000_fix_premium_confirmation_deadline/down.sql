do $$ begin
    if exists (select 1 from premium_renewal_agreements) or exists (select 1 from premium_legacy_renewals) then
        raise exception 'Retained Premium evidence prevents removing immutable confirmation deadlines';
    end if;
end $$;
alter table premium_renewal_agreements
    drop constraint premium_renewal_activation_snapshot,
    drop column paid_period_id,
    drop column confirmation_deadline;
