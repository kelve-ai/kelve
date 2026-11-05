-- The person's skills (docs/skills.md): each a folder with a SKILL.md, the
-- file the source of truth. kelve keeps here only what isn't in the file:
-- what kind of skill each is, and, for each part, what its checks found.
create table skills (
    id integer primary key autoincrement,
    -- The skill's folder name, as its front matter names it.
    name text not null,
    -- How you like it, how you do something, or one project's.
    kind text not null check (kind in ('like', 'do', 'project')),
    -- For a `like` skill, the kind of work it's for (`core`'s list:
    -- `everywhere`, `rust`, `words`…); '' for the others.
    work text not null default '',
    -- The project it's in, absolute, for a `project` skill; '' for the
    -- person's own.
    root text not null default '',
    -- `made`: a skill the person has. `suggested`: drafted by the learn
    -- job, waiting for the person. `not_now`: a suggestion they turned down.
    state text not null default 'made' check (state in ('made', 'suggested', 'not_now')),
    -- A suggestion's draft in full, and why it's suggested: none once made,
    -- when the file is the skill.
    draft text,
    why text,
    made_at integer not null,
    updated_at integer not null,
    unique (root, name)
) strict;
create index skills_kind on skills (kind, work);

-- A part of a skill: a `###` heading and everything under it. Known by its
-- heading, normalised: an edited heading is a new part.
create table parts (
    id integer primary key autoincrement,
    skill integer not null references skills (id) on delete cascade,
    -- The heading, normalised (`core`'s part key).
    key text not null,
    -- The heading as written.
    heading text not null,
    -- The section it's under: `like` (How you like it), `steps`, `watch`
    -- (Watch out for), or '' for none.
    section text not null default '',
    -- A hash of its text as last read: one that changed is checked again.
    body_hash text not null,
    state text not null default 'unchecked'
        check (state in ('unchecked', 'running', 'works', 'no_difference', 'failed')),
    checked_at integer,
    -- What the last check found, in a line, and the clearer wording the
    -- agent suggested when the part failed.
    note text,
    wording text,
    -- The person kept it though it made no difference.
    kept integer not null default 0,
    seen_at integer not null,
    -- When it was last seen gone from the file, or removed from kelve.
    removed_at integer,
    unique (skill, key)
) strict;
