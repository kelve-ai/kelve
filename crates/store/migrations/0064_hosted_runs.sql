-- The verified runs this kelve asked kelve.ai for (docs/skills.md, "Proof:
-- two scores"), so one under way can be gone back to: what was run and how
-- it stands, as kelve.ai last said. Never the skills that went, or the key
-- that paid for it.
create table hosted_runs (
    -- kelve.ai's ID for it.
    id text primary key,
    kind text not null,
    kind_name text not null,
    agent text not null,
    agent_name text not null,
    model text not null,
    model_name text not null,
    -- Whose key pays for it: `anthropic`.
    provider text not null,
    provider_name text not null,
    state text not null default 'estimated'
        check (state in ('estimated', 'queued', 'running', 'finished', 'cancelled', 'failed')),
    -- The score it made, once it's finished.
    score text,
    at integer not null
) strict;
create index hosted_runs_at on hosted_runs (at);
