-- What an agent writes through its shell (docs/specs/lessons.md, 4).

-- Each session's last moment: files written after it, at the next shell
-- command, are what that command wrote. Kept a day.
create table moments (
    session text primary key,
    at integer not null
) strict;

-- Each file as kelve last saw it in a session, masked: so a change that's
-- only whitespace, as a formatter makes, isn't taken for writing. Kept a day.
create table files_seen (
    session text not null,
    root text not null,
    path text not null,
    text text not null,
    at integer not null,
    primary key (session, root, path)
) strict;
create index files_seen_at on files_seen (at);
