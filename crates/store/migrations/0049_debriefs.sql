-- A debrief for each piece of work (docs/skills.md): what the agent did,
-- how it works and what it decided, written by the agent as one markdown
-- file, and the person's call on each decision.
create table debriefs (
    id integer primary key autoincrement,
    -- The handover it's of: the latest, as the same work is brought up to
    -- date.
    handover integer references handovers (id) on delete set null,
    root text not null default '',
    session text not null default '',
    state text not null default 'waiting'
        check (state in ('waiting', 'writing', 'ready', 'sent', 'failed')),
    title text not null default '',
    -- How big the change is, as kelve shows it: `small` or `large`, '' till
    -- it's written.
    size text not null default '',
    markdown text,
    -- How many times the agent wrote it, and what was wrong the last time.
    tries integer not null default 0,
    last_error text,
    made_at integer not null,
    updated_at integer not null,
    sent_at integer
) strict;
create index debriefs_state on debriefs (state, updated_at);
create index debriefs_handover on debriefs (handover);

-- A decision in a debrief, by its number there, and the person's call.
create table decisions (
    id integer primary key autoincrement,
    debrief integer not null references debriefs (id) on delete cascade,
    number integer not null,
    title text not null,
    -- Where it came from: the agent's own call, what the person said, or
    -- their edit after handover.
    origin text not null check (origin in ('own', 'said', 'edit')),
    -- Its lines in the change, as JSON: [{path, from, to}].
    lines text not null default '[]',
    -- The part of the change it belongs to, on a large change.
    part text not null default '',
    choice text check (choice in ('agree', 'change', 'look')),
    note text,
    -- The person wants it this way every time: into which skill.
    every_time integer not null default 0,
    skill integer references skills (id) on delete set null,
    chosen_at integer,
    unique (debrief, number)
) strict;

-- Research the person asked for on a decision, and the agent's answer.
create table research (
    id integer primary key autoincrement,
    decision integer not null references decisions (id) on delete cascade,
    state text not null default 'waiting'
        check (state in ('waiting', 'running', 'ready', 'failed')),
    question text not null,
    answer text,
    asked_at integer not null,
    answered_at integer
) strict;
create index research_decision on research (decision);

-- An explanation of an idea the work rests on, kept so the same idea isn't
-- explained twice for the same code.
create table explanations (
    id integer primary key autoincrement,
    debrief integer references debriefs (id) on delete set null,
    -- The idea's name, and that name normalised.
    idea text not null,
    key text not null,
    -- A hash of the lines that use it.
    print text not null,
    state text not null default 'waiting'
        check (state in ('waiting', 'running', 'ready', 'failed')),
    answer text,
    asked_at integer not null,
    answered_at integer,
    unique (key, print)
) strict;
