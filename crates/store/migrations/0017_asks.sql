-- The work kelve asks the person's own agent to do (docs/specs/agent.md,
-- section 5): one row a job, until the agent hands in its answer and the
-- work that asked for it has taken it. `key` names what's asked, so asking
-- again for the same thing is one job. `material` and `schema` are JSON:
-- what the agent is given, and the shape of its answer. `resume_*` is the
-- background job to run again once it's answered.
create table asks (
    id integer primary key autoincrement,
    key text not null unique,
    kind text not null,
    -- What it's for, in a few words: "Reading anthropic.rs".
    about text not null,
    idea text,
    opened integer,
    material text not null,
    schema text not null,
    resume_kind text not null,
    resume_key text,
    resume_payload text not null default '{}',
    -- 0 for what the person is looking at, 1 for what they kept for their
    -- agent, 2 for the rest of what they opened.
    priority integer not null default 2,
    -- 'waiting', 'taken', 'answered' or 'dropped'.
    state text not null default 'waiting'
        check (state in ('waiting', 'taken', 'answered', 'dropped')),
    answer text,
    model text,
    agent text,
    -- Answers that weren't in the shape asked for.
    tries integer not null default 0,
    last_error text,
    taken_at integer,
    answered_at integer,
    created_at integer not null default (cast(unixepoch('subsec') * 1000 as integer))
) strict;
create index asks_waiting on asks (state, priority, id);

-- The agents and plans that have done kelve's work, and the model each
-- last said it ran on: what goes to the graph with what they wrote.
create table agents (
    name text primary key,
    model text,
    plan text,
    first_seen_at integer not null,
    last_seen_at integer not null
) strict;
