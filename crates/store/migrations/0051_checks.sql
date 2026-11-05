-- The person's past work, as a skill check or Fits you redoes it: what they
-- asked, where it started, and what they kept (docs/skills.md).
create table past_tasks (
    id integer primary key autoincrement,
    -- What they asked, short, secrets stripped.
    request text not null,
    root text not null,
    -- The commit the work started from.
    start text not null,
    -- What they kept: a commit, or a tree; none till it's known.
    kept text,
    handover integer references handovers (id) on delete set null,
    debrief integer references debriefs (id) on delete set null,
    at integer not null
) strict;
create index past_tasks_root on past_tasks (root, at);

-- A part checked on a past task: redone with and without it.
create table checks (
    id integer primary key autoincrement,
    part integer not null references parts (id) on delete cascade,
    task integer not null references past_tasks (id) on delete cascade,
    state text not null default 'waiting'
        check (state in ('waiting', 'running', 'done', 'failed', 'stopped')),
    verdict text check (verdict in ('works', 'no_difference', 'failed')),
    notes text,
    -- A clearer wording, when the part failed.
    wording text,
    asked_at integer not null,
    done_at integer
) strict;
create index checks_part on checks (part, state);

-- Fits you: a past task redone by the agent with the person's skills and by
-- a standard one, shown side by side, blind; which they'd rather keep.
create table fits_picks (
    id integer primary key autoincrement,
    task integer not null references past_tasks (id) on delete cascade,
    -- Which side the person's agent was shown on.
    skills_on text not null check (skills_on in ('left', 'right')),
    picked text check (picked in ('left', 'right', 'same')),
    -- How close each came to what the person kept, from 0 to 1.
    skills_close real,
    standard_close real,
    made_at integer not null,
    picked_at integer
) strict;
create index fits_picks_task on fits_picks (task);
