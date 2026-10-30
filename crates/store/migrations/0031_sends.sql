-- Each time an entry the person's agent wrote was sent to the shared graph,
-- and how the graph's check of it stands: what settings say was sent today.
-- A fix sent again after the checks turned it away is a send of its own.
create table sends (
  id integer primary key,
  idea text not null,
  -- The revision the graph gave it, unless it was turned away at once.
  revision text,
  state text not null check (state in ('checking', 'accepted', 'turned_away')),
  sent_at integer not null
) strict;
create index sends_at on sends (sent_at);
create index sends_revision on sends (revision);

-- While the account has added as much as the shared graph takes a day:
-- nothing more is sent until `until`, when the graph's day's limits reset,
-- even after a restart. `what` ran out: 'ideas', 'entries', 'relations',
-- or '' when the graph didn't say.
create table graph_held (
  id integer primary key check (id = 1),
  until integer not null,
  what text not null,
  new_account integer not null
) strict;
