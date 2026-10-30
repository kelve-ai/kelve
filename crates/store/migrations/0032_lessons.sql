-- Lessons (docs/specs/lessons.md): what people's agents learned using the
-- ideas kelve marks, from the shared graph, as last fetched.
create table lessons (
    id text primary key,
    -- The graph's page for it, as JSON.
    page text not null,
    fetched_at integer not null
) strict;

-- Which lesson is about which idea, by the idea's key.
create table lesson_ideas (
    lesson text not null references lessons (id) on delete cascade,
    idea text not null,
    primary key (idea, lesson)
) strict;

-- Each idea whose lessons were fetched, and when: fetched again a day on.
create table lessons_fetched (
    idea text primary key,
    fetched_at integer not null
) strict;

-- Each lesson given to the person's agent, and what came of it. Kept here
-- only: just the lesson, what came of it and what showed it go to the
-- graph, never the problem, the command or the file.
create table given (
    id integer primary key,
    lesson text not null,
    -- The agent's session, as its hook names it.
    session text not null,
    -- When: as it read or edited a file, as something failed, or as it asked.
    moment text not null check (moment in ('read', 'failed', 'asked')),
    root text not null default '',
    path text not null default '',
    -- For a failure: the problem, as kelve saw it, and the command.
    problem text not null default '',
    command text not null default '',
    given_at integer not null,
    outcome text not null default 'unknown' check (outcome in ('held', 'didnt', 'unknown')),
    -- What showed it: `passed`, `cameBack`, `said` or `nothing`.
    shown text not null default 'nothing',
    -- What the agent said, when it said.
    said text not null default '',
    sent integer not null default 0
) strict;
create index given_session on given (session, lesson);
create index given_root on given (root, path);

-- What failed as the agent worked, until the same command passes: what
-- a lesson is written from.
create table failed (
    id integer primary key,
    session text not null,
    root text not null,
    command text not null,
    -- As kelve saw it: the first line, with the work's own names out.
    problem text not null,
    -- The files the agent read or edited in the session, as JSON, by path
    -- in the root: their ideas are what a lesson can be about.
    files text not null default '[]',
    failed_at integer not null,
    passed_at integer
) strict;
create index failed_session on failed (session, command);

-- What the person's agent learned, held here a day before it leaves, so
-- they can stop it.
create table learned (
    id integer primary key,
    -- The lesson, as JSON, about the graph's IDs.
    lesson text not null,
    -- The names of the ideas it's about, as JSON, for showing.
    names text not null,
    state text not null check (state in
        ('waiting', 'stopped', 'checking', 'live', 'held', 'same', 'turnedAway')),
    leaves_at integer not null,
    root text not null default '',
    -- The graph's ID, once sent; and the lesson it's the same as.
    graph_id text,
    same_as text,
    -- Why the graph turned it away, or what it's waiting for.
    why text,
    written_at integer not null
) strict;
create index learned_leaves on learned (state, leaves_at);
