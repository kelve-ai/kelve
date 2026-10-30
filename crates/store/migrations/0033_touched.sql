-- The files the person's agent read or edited lately, by its session: what
-- a lesson it writes can be about, and where a fix went (docs/specs/
-- lessons.md). Kept a day, so a restart of kelve doesn't forget them.
create table touched (
    session text not null,
    root text not null,
    path text not null,
    edited integer not null default 0,
    at integer not null,
    primary key (session, root, path)
) strict;
create index touched_at on touched (at);

-- What kelve has for an agent that can't be given it at the moment it
-- happened (Antigravity), until the agent next calls its model.
create table held_for_agent (
    id integer primary key,
    session text not null,
    text text not null,
    at integer not null
) strict;
create index held_for_agent_session on held_for_agent (session);

-- A lesson that fitted but was held back from the agent, one in twenty, to
-- measure the lessons; and how likely each was given, in thousandths.
alter table given add column withheld integer not null default 0;
alter table given add column chance integer not null default 1000;
