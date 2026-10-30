-- The ideas whose entries the person has opened: each square they've read
-- is filled (docs/specs/reader.md, 3.5). Kept here only, never sent.
create table ideas_read (
    idea text primary key,
    first_read integer not null
) strict;
