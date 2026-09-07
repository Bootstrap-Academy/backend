-- The designation of the contract as the consumer wrote it
-- (§ 312k Abs. 2 S. 2 Nr. 2 BGB), and the note an administrator leaves when
-- the declaration has been processed by hand.
alter table contract_declarations
    add column contract_designation text,
    add column processing_note text;
