-- The relations the person followed, from one idea's entry to another
-- (docs/specs/agent.md, 9): each kept once, ever, so it's counted once, and
-- sent to the shared graph in a batch with nothing about who followed it.
create table follows (
    from_idea text not null,
    kind text not null,
    to_idea text not null,
    -- 0 until it's gone to the shared graph.
    sent integer not null default 0,
    primary key (from_idea, kind, to_idea)
) strict;
