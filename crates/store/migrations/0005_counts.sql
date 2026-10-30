-- How kelve was used, counted a day at a time, for the weekly count the
-- person may choose to share: numbers only, never what was opened, read
-- or written.
create table counts (
    -- The person's own date: 2026-10-15.
    day text not null,
    -- What it counts: opened.change, links.followed.
    what text not null,
    n integer not null default 0,
    primary key (day, what)
) strict;

-- What's counted once a day already, such as an idea reached by a link.
-- Only so a page fetched again isn't counted again; kept a fortnight.
create table counted (
    day text not null,
    what text not null,
    key text not null,
    primary key (day, what, key)
) strict;

-- Each week's count, as it was sent.
create table counts_sent (
    -- The Monday the week began.
    week text primary key,
    report text not null,
    sent_at integer not null
) strict;
