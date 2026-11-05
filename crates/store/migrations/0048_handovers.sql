-- What the person's agent handed over as it showed its work in kelve, as
-- kelve read it then: compared with what's committed later, what the person
-- changed in the agent's work is the clearest sign of what they know.
create table handovers (
    id integer primary key autoincrement,
    -- What it opened, while that's kept.
    opened integer references opened (id) on delete set null,
    -- The repository, absolute.
    root text not null,
    -- The branch checked out, or '' when none is.
    branch text not null default '',
    -- The branch it was shown against, or '' when none.
    base text not null default '',
    -- How it was shown: `since main`, `uncommitted`, `commit <rev>`.
    change text not null,
    -- The commit checked out then, in full, or '' for none.
    head text not null default '',
    -- Whether some of it wasn't committed.
    uncommitted integer not null default 0,
    agent text not null default '',
    -- The agent's session, as its hook names it, once the hook says.
    session text not null default '',
    title text not null default '',
    said text not null default '',
    files integer not null default 0,
    added integer not null default 0,
    removed integer not null default 0,
    -- Whether more was handed over than kelve kept.
    cut integer not null default 0,
    at integer not null,
    -- When a commit was last compared with it.
    compared_at integer
) strict;
create index handovers_root on handovers (root, at);
create index handovers_opened on handovers (opened);

-- Each file handed over: its status, its hunks with secrets masked, and,
-- when it isn't too big, its text as handed over, masked too.
create table handover_files (
    handover integer not null references handovers (id) on delete cascade,
    path text not null,
    status text not null check (status in ('added', 'modified', 'deleted')),
    moved_from text,
    -- Why it wasn't read, when it wasn't: a secret, a lockfile, not text.
    skip text,
    added integer not null default 0,
    removed integer not null default 0,
    -- Its hunks, as JSON.
    hunks text not null default '[]',
    text text,
    primary key (handover, path)
) strict;

-- What the person said to their agent, as its hook saw each message:
-- short, secrets stripped, kept a week, for catching what they corrected.
create table messages (
    id integer primary key autoincrement,
    session text not null,
    agent text not null default '',
    root text not null default '',
    text text not null,
    at integer not null
) strict;
create index messages_session on messages (session, at);
create index messages_at on messages (at);
