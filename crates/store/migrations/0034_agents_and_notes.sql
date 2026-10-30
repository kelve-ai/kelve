-- Which agent each lesson was given to, as its hook names it: the reader
-- names the agent that was given it, not the one seen last.
alter table given add column agent text not null default '';

-- The notes left for the agent that its hook told it of, and at which
-- version of their words (docs/specs/reader.md, 8.5): the hook's one line
-- counts only notes new or changed since.
create table notes_told (
    agent text not null,
    root text not null,
    note text not null,
    version text not null,
    at integer not null,
    primary key (agent, root, note)
) strict;
