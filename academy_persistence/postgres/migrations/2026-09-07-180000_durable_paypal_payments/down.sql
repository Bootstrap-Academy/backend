-- Binary recovery keeps this additive schema. Never remove recorded payment obligations.
do $$ begin
    if exists (select 1 from paypal_payments) or exists (select 1 from paypal_legacy_reconciliation) then
        raise exception 'Refusing to remove PayPal payment evidence; use a compatible binary or forward repair';
    end if;
end $$;
drop table paypal_legacy_reconciliation;
drop table paypal_payments;
drop function protect_paypal_payment();
