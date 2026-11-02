-- What each of the person's agent's sessions opened in kelve, and when, as
-- its hook saw it: as its turn ends, it's asked to finish reading what it
-- opened (docs/architecture.md). Kept a day.
create table opened_in (
    session text not null,
    opened integer not null,
    at integer not null,
    primary key (session, opened)
) strict;
create index opened_in_at on opened_in (at);
