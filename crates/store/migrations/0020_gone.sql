-- What the first version no longer keeps: the hand-off to an agent, the
-- first read's own record, and the daily spending ledger from when kelve
-- called models itself.
drop table if exists handoffs;
drop table if exists first_reads;
drop table if exists ledger;
