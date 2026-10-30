-- What kelve last heard of its newest release, asked at most once a day,
-- even across restarts: when it last asked, and the newest version with
-- the first line of its notes. And the newest version the person's agent
-- was told of, so it's told once for each.
create table updates (
  id integer primary key check (id = 1),
  checked_at integer not null,
  newest text,
  line text,
  told text
) strict;
