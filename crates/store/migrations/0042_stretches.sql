-- What failed is asked about once a turn ends, as one hard stretch, not by
-- pairing each failure with the command that passed it (docs/specs/
-- lessons.md, 4): when kelve settled it, and whether it asked; and when a
-- command last passed in each session, so a turn that ends still failing
-- isn't asked.
alter table failed add column settled_at integer;
alter table failed add column asked integer not null default 0;
alter table moments add column passed_at integer;
