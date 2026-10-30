-- The libraries each file read imports, by the name each is published
-- under: what lessons about a library are found by (docs/specs/lessons.md),
-- though kelve marks what a library does, not the library.
create table uses (
    root text not null,
    path text not null,
    name text not null,
    primary key (root, path, name)
) strict;

-- Which of the shared graph's ideas a library is, by its name: none when
-- the graph has none that's surely it. Looked up again after a day.
create table libraries (
    name text primary key,
    idea text,
    looked_at integer not null
) strict;
