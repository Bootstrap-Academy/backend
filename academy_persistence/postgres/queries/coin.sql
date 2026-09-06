--: Balance()
--: Transaction(description?)

--! get_balance : Balance
select coins, withheld_coins from coins where user_id=:user_id;

--! add_coins : Balance
merge into coins
  using (select :user_id::uuid as user_id) as u
  on coins.user_id=u.user_id
  when not matched then insert (user_id, coins, withheld_coins) values (:user_id, :coins, :withheld_coins)
  when matched then update set coins=coins+:coins, withheld_coins=withheld_coins+:withheld_coins
  returning coins, withheld_coins;

--! release_coins
update coins set coins=coins+withheld_coins, withheld_coins=0 where user_id=:user_id;

--! list_transactions : Transaction
select * from transactions
  where user_id=:user_id
    and :start <= created_at
    and created_at < :end
  order by created_at asc;

--! list_all_transactions : Transaction
select * from transactions where user_id=:user_id order by created_at asc;

--! create_transaction (description?)
insert into transactions (id, user_id, created_at, coins, description, include_in_credit_note)
  values (:id, :user_id, :created_at, :coins, :description, :include_in_credit_note);
