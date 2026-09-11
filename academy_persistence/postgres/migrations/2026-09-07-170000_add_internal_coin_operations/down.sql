-- Removing replay evidence after use would permit duplicate credits.
do $$ begin
    if exists (select 1 from internal_coin_operations) then
        raise exception 'Cannot remove nonempty coin operation evidence; use a forward repair';
    end if;
end $$;
drop table internal_coin_operations;
