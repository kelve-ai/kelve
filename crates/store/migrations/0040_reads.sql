-- How many times the shared graph's entry for each idea has been read, by
-- anyone, as the graph last said: for the line under an entry that came
-- from it (docs/specs/reader.md, 3.6). Nothing about who.
alter table entries add column reads integer not null default 0;
