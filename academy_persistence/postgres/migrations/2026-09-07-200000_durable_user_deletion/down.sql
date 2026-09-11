do $$ begin
    if exists (select 1 from user_deletion_work) then
        raise exception 'Cannot discard pending account erasure work; keep the additive schema';
    end if;
end $$;
drop trigger queue_user_deletion on users;
drop function queue_user_deletion();
drop table user_deletion_work;
