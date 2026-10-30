-- Better ways as well as fixes (docs/specs/lessons.md, 3 and 4).

-- What the agent wrote in a session, file by file: so kelve can tell when
-- an edit replaces the agent's own earlier work. Kept a day.
create table written (
    id integer primary key,
    session text not null,
    root text not null,
    path text not null,
    text text not null,
    at integer not null
) strict;
create index written_session on written (session, root, path);
create index written_at on written (at);

-- Work the agent replaced, its first way and its second, waiting until the
-- second way stands to ask for a lesson: the next command that passes, or
-- the person's next message.
create table replaced (
    id integer primary key,
    session text not null,
    root text not null,
    path text not null,
    first text not null,
    second text not null,
    at integer not null,
    asked integer not null default 0
) strict;
create index replaced_session on replaced (session);

-- A lesson given as the agent wrote the usual way it replaces, and the line
-- it wrote; and whether that line then went, done the lesson's way, which
-- the commit settles.
alter table given add column instead integer not null default 0;
alter table given add column usual text not null default '';
alter table given add column done integer not null default 0;

-- What the agent learned from replaced work: its first way and its second,
-- which never leave.
alter table learned add column first text;
alter table learned add column second text;
alter table learned add column path text;
