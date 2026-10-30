-- The ideas kelve knows about: the shared graph's, as last fetched, and
-- ideas the person's kelve came across that the graph doesn't have yet.
create table ideas (
    -- The graph's ID, or `new:` and the name as names are matched.
    key text primary key,
    name text not null,
    definition text not null,
    kind text not null,
    -- For a new idea, the graph's ID once it has one.
    became text,
    updated_at integer not null
) strict;

-- Each idea's entry: written here, or taken from the shared graph.
create table entries (
    idea text primary key references ideas (key),
    state text not null
        check (state in ('planning', 'writing', 'checking', 'ready', 'waiting', 'dropped')),
    -- What it's doing now, or why it waits or was dropped, in a sentence.
    step text not null default '',
    plan text,
    entry text,
    claims text,
    -- Taken from the shared graph, as this revision; or, once sent there,
    -- the revision it became.
    revision text,
    from_graph integer not null default 0,
    sent integer not null default 0,
    cost real not null default 0,
    created_at integer not null,
    updated_at integer not null
) strict;
create index entries_state on entries (state);
