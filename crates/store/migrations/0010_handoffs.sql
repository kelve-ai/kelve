-- Handing the note for coding agents to the person's own agent: when it
-- went, to which agent, for which folder, and when the agent said it was
-- done, with what it said. What the folder held when it went is kept
-- beside kelve's data, as files (`server::handoff`).
create table handoffs (
    id integer primary key autoincrement,
    root text not null,
    agent text not null,
    sent_at integer not null,
    done_at integer,
    said text,
    reviewed_at integer
) strict;
create index handoffs_root on handoffs (root, id);
