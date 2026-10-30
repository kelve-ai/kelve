-- An entry taken from the shared graph that its checks later found wrong
-- (spec 8.3): what they found, and, while kelve writes one to replace it,
-- the entry and revision it replaces, still shown until then.

-- What the graph's checks found wrong with the revision kept, as JSON.
alter table entries add column problems text;
-- The entry shown while its replacement is written, as JSON.
alter table entries add column replacing text;
-- The revision the replacement replaces.
alter table entries add column replaces text;
-- When the graph was last asked about the entry.
alter table entries add column checked_at integer;
