-- Folders kelve's hook met the person's agent in that the person never
-- opened (docs/specs/lessons.md, 2): walked once for what each file
-- imports, so lessons reach the agent there. Never in the reader: not
-- opened, nothing read or marked, nothing on the agent's list.
create table walked_roots (
    root text primary key,
    walked_at integer not null,
    -- How many files the walk found, and whether it stopped at its cap.
    files integer not null default 0,
    cut integer not null default 0
) strict;

-- The files a walk found, by path in its folder, each with what it imports
-- kept in `uses`, as a file the reader reads.
create table walked_files (
    root text not null,
    path text not null,
    primary key (root, path)
) strict;
