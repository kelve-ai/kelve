-- What the person has opened: a folder, a file, or a change in a repo.
-- Opening the same thing again brings it to the top.
create table opened (
    id integer primary key autoincrement,
    kind text not null check (kind in ('folder', 'file', 'change')),
    -- The folder it's in, absolute.
    root text not null,
    -- A file's path in the folder; '' for a folder or a change.
    path text not null default '',
    -- Which change: 'uncommitted', 'against <base>', 'commit <rev>'; '' else.
    change text not null default '',
    title text not null,
    opened_at integer not null,
    unique (kind, root, path, change)
) strict;
create index opened_at on opened (opened_at);

-- The ideas each file kelve has read is built on, by the folder it's in:
-- what tells an idea in a change is new to the project.
create table read_files (
    root text not null,
    path text not null,
    -- The keys and names of the ideas marked in it, as JSON.
    ideas text not null,
    read_at integer not null,
    primary key (root, path)
) strict;

-- What a folder is built on, as chosen from the ideas marked in its files,
-- kept against a fingerprint of those ideas.
create table built_on (
    fingerprint text primary key,
    chosen text not null,
    made_at integer not null
) strict;
