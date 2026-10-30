-- What the person asked for by name in something opened, for "Ready for
-- you" (docs/specs/reader.md, 3.13): an entry, by its idea's key, or a
-- "why", by what it's about (`why::key`, with the place as JSON). It's
-- ready once there's something to read, and leaves the line once seen.
create table asked (
  opened integer not null,
  kind text not null check (kind in ('entry', 'why')),
  target text not null,
  data text not null default '',
  asked_at integer not null,
  seen_at integer,
  primary key (opened, kind, target)
);
