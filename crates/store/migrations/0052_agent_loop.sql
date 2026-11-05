-- The agent loop (docs/skills.md): whether each handover was worth a
-- debrief, and why; the commit last compared with it; evidence once the
-- learn job has had it; each skill given to an agent's session through the
-- hook, once; and each change kelve made to a skill file, so it can be undone.

-- Whether a debrief was due for what was handed over, and why, in a
-- sentence for home: null till kelve decided.
alter table handovers add column due integer;
alter table handovers add column due_why text not null default '';
-- The commit kelve last compared with what was handed over, so the same
-- commit isn't compared twice.
alter table handovers add column compared_head text not null default '';

-- The part a candidate goes after in its section, by its heading as written,
-- so it lands next to what it's about; none for the section's end.
alter table candidates add column after_part text;

-- When the learn job was given this evidence: what it learned from, whether
-- or not it made a part of it.
alter table evidence add column learned_at integer;

-- A skill given to an agent's session through the hook: once a session.
create table skills_given (
    session text not null,
    -- The skill, as `<root>|<name>`: '' for the person's own.
    skill text not null,
    at integer not null,
    primary key (session, skill)
) strict;
create index skills_given_at on skills_given (at);

-- A change kelve made to a skill's file, with the text before and after, so
-- the person can undo it.
create table skill_edits (
    id integer primary key autoincrement,
    skill integer references skills (id) on delete set null,
    candidate integer references candidates (id) on delete set null,
    -- Where the file is: the project, or '' for the person's own; and the
    -- skill's name.
    root text not null default '',
    name text not null,
    -- `new` for a part added, `replaced` for one put in place of another.
    relation text not null check (relation in ('new', 'replaced')),
    heading text not null,
    -- The part it replaced, for undoing it where the file has changed since.
    old_heading text,
    old_body text,
    before text not null,
    after text not null,
    at integer not null,
    undone_at integer
) strict;
create index skill_edits_candidate on skill_edits (candidate);
