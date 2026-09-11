-- The operator authorized default-OFF for every existing subscription. Preserve
-- observations, not invented contract acceptance or historic prices. Paid access
-- and the coin ledger are deliberately untouched.
create table premium_legacy_renewals (
    user_id uuid primary key,
    plan premium_plan not null,
    archived_at timestamptz not null default current_timestamp,
    paid_periods jsonb not null,
    observed_terms_version text,
    observed_terms_accepted_at timestamptz,
    observed_terms_declined_at timestamptz
);
insert into premium_legacy_renewals
    (user_id, plan, paid_periods, observed_terms_version, observed_terms_accepted_at, observed_terms_declined_at)
select s.user_id, s.plan,
    coalesce((select jsonb_agg(to_jsonb(p) order by p.since) from premium p where p.user_id=s.user_id), '[]'),
    u.terms_version, u.terms_accepted_at, u.terms_declined_at
from premium_subscriptions s join users u on u.id=s.user_id;
delete from premium_subscriptions;

create table premium_renewal_agreements (
    id uuid primary key,
    user_id uuid not null,
    received_at timestamptz not null,
    offer_id text not null,
    monthly_price bigint not null check (monthly_price > 0),
    recipient text not null,
    document text not null,
    terms_pdf bytea not null,
    withdrawal_pdf bytea not null
);
create index premium_renewal_agreements_user on premium_renewal_agreements(user_id);
create table premium_renewal_delivery (
    agreement_id uuid primary key references premium_renewal_agreements(id) on delete cascade,
    attempts bigint not null default 0,
    last_attempt_at timestamptz,
    sent_at timestamptz
);
create table premium_renewal_cancellations (
    agreement_id uuid primary key references premium_renewal_agreements(id) on delete cascade,
    cancelled_at timestamptz not null default current_timestamp,
    paid_until timestamptz
);
alter table premium_subscriptions add column agreement_id uuid references premium_renewal_agreements(id);

create function preserve_premium_renewal_evidence() returns trigger language plpgsql as $$
begin
    raise exception 'Premium renewal evidence is immutable; create a new declaration';
end;
$$;
create trigger immutable_premium_legacy_renewals before update on premium_legacy_renewals
    for each row execute function preserve_premium_renewal_evidence();
create trigger immutable_premium_renewal_agreements before update on premium_renewal_agreements
    for each row execute function preserve_premium_renewal_evidence();
create trigger immutable_premium_renewal_cancellations before update on premium_renewal_cancellations
    for each row execute function preserve_premium_renewal_evidence();

create function end_premium_renewal_on_user_deletion() returns trigger language plpgsql as $$
begin
    insert into premium_renewal_cancellations (agreement_id, paid_until)
    select agreement_id, (select max(until) from premium where user_id=old.id)
    from premium_subscriptions where user_id=old.id and agreement_id is not null
    on conflict do nothing;
    return old;
end;
$$;
create trigger end_premium_renewal_on_user_deletion before delete on users
    for each row execute function end_premium_renewal_on_user_deletion();
