-- A final statement records the unused share of the purchased Morphcoins so
-- that it can be refunded on request after the account has been deleted. There
-- was nothing on the record that said whether that refund had been made, and
-- after the deletion there is no balance left to check either, so the same
-- statement could have been handed in twice.
--
-- `settled_at` records when the claim was closed out. Refunds are made by
-- hand, so the timestamp is set by hand as well, with
-- `academy admin finance settle <number>` once the money has been sent.
alter table financial_documents add column settled_at timestamp with time zone;
