-- Why it's this way is gone: reading plainly replaced it
-- (docs/specs/plainly.md). What kept its answers, what was asked for it
-- and what waits for the agent to write one go with it. What's asked for
-- by name is an idea's entry, by its key.
drop table if exists kept_why;
delete from asks where kind = 'why';
delete from jobs where kind = 'why.write';

create table asked_entries (
  opened integer not null,
  kind text not null check (kind = 'entry'),
  target text not null,
  asked_at integer not null,
  seen_at integer,
  primary key (opened, kind, target)
);
insert into asked_entries (opened, kind, target, asked_at, seen_at)
  select opened, kind, target, asked_at, seen_at from asked where kind = 'entry';
drop table asked;
alter table asked_entries rename to asked;
