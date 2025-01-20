--! get
select hearts, last_refill from hearts where user_id=:user_id;

--! set
insert into hearts (user_id, hearts, last_refill)
  values (:user_id, :hearts, :last_refill)
  on conflict (user_id) do update set hearts=:hearts, last_refill=:last_refill;
