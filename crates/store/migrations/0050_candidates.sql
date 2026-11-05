-- A candidate: one proposed part for a skill, from what the person said,
-- changed or decided, waiting to go into a skill (docs/skills.md).
create table candidates (
    id integer primary key autoincrement,
    -- The skill it goes in: one the person has, or one suggested for it.
    -- None till kelve has placed it.
    skill integer references skills (id) on delete cascade,
    -- The kind of work it was caught in, where no skill is chosen yet.
    work text not null default '',
    section text not null default '' check (section in ('', 'like', 'steps', 'watch')),
    heading text not null,
    -- The part in full, in markdown.
    body text not null,
    -- The part it says again, disagrees with or rewords, when it does.
    part integer references parts (id) on delete set null,
    relation text not null default 'new'
        check (relation in ('new', 'same', 'disagrees', 'rewords')),
    state text not null default 'waiting'
        check (state in ('waiting', 'proposed', 'accepted', 'dismissed')),
    -- The debrief it's shown in, under For your skills.
    debrief integer references debriefs (id) on delete set null,
    made_at integer not null,
    decided_at integer
) strict;
create index candidates_state on candidates (state, made_at);
create index candidates_skill on candidates (skill);

-- What a part or a candidate rests on: a message the person sent, their
-- edit after a handover, or their call on a decision. Short, secrets
-- stripped; it never leaves.
create table evidence (
    id integer primary key autoincrement,
    kind text not null check (kind in ('message', 'edit', 'decision')),
    part integer references parts (id) on delete set null,
    candidate integer references candidates (id) on delete set null,
    session text not null default '',
    root text not null default '',
    -- For an edit: the handover and the file.
    handover integer references handovers (id) on delete set null,
    path text not null default '',
    decision integer references decisions (id) on delete set null,
    text text not null,
    at integer not null
) strict;
create index evidence_part on evidence (part);
create index evidence_candidate on evidence (candidate);
create unique index evidence_edit on evidence (handover, path) where kind = 'edit';
