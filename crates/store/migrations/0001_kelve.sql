-- kelve's local state, on the person's own computer: one SQLite file.
-- What they open never lands here, only what kelve worked out from it.
-- Times are milliseconds since 1970, UTC, so they compare as numbers;
-- days are 'YYYY-MM-DD' in the person's time zone; money is US dollars.

-- The person's settings, as one JSON document.
create table instance (
    id integer primary key check (id = 1),
    settings text not null default '{}',
    updated_at integer not null default (cast(unixepoch('subsec') * 1000 as integer))
) strict;
insert into instance (id) values (1);

-- Keys pasted in the app, sealed before they get here.
create table secrets (
    name text primary key,
    value blob not null,
    updated_at integer not null default (cast(unixepoch('subsec') * 1000 as integer))
) strict;

-- Signing in a browser to `kelve serve`: one-time links, and the browsers
-- they signed in. Only hashes.
create table sign_in_links (
    hash blob primary key,
    expires_at integer not null,
    used_at integer
) strict;

create table browser_sessions (
    hash blob primary key,
    created_at integer not null default (cast(unixepoch('subsec') * 1000 as integer)),
    last_seen_at integer not null default (cast(unixepoch('subsec') * 1000 as integer)),
    expires_at integer
) strict;

-- Background work: one row a job, debounced by key.
create table jobs (
    id integer primary key autoincrement,
    kind text not null,
    key text unique,
    payload text not null default '{}',
    run_at integer not null,
    attempts integer not null default 0,
    locked_at integer,
    -- Asked for again while it ran: it runs again then, instead of going.
    again_at integer,
    last_error text,
    created_at integer not null default (cast(unixepoch('subsec') * 1000 as integer))
) strict;
create index jobs_ready on jobs (run_at) where locked_at is null;

-- What kelve spends on the person's keys: reserved before a call, settled
-- after.
create table ledger (
    id integer primary key autoincrement,
    day text not null,
    job text not null,
    model text,
    reserved real not null default 0,
    spent real not null default 0,
    settled integer not null default 0,
    batched integer not null default 0,
    searches integer not null default 0,
    at integer not null default (cast(unixepoch('subsec') * 1000 as integer))
) strict;
create index ledger_day on ledger (day);
