-- Entries kelve writes ahead, before anyone asks, for the ideas the person
-- is likely to open next: sent as batches, at half the price, when the
-- person wants that. Asked for meanwhile, one is written at once instead.
alter table entries add column ahead integer not null default 0;
-- The batch a step is waiting on, and what was reserved for it, as JSON.
alter table entries add column pending text;
