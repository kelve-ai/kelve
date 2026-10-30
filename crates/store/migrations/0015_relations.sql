-- How each idea relates (docs/specs/graph.md), kept to draw its outline:
-- what the shared graph served, or what this kelve proposed when the graph
-- had none. `relations` is the graph API's IdeaRelations, as JSON, its
-- ideas by kelve's keys. `question` is the one the idea answers.
create table idea_relations (
    key text primary key,
    relations text not null,
    -- 'graph': served by the shared graph. 'proposed': proposed here.
    source text not null check (source in ('graph', 'proposed')),
    question text not null default '',
    -- What was proposed, as the graph's changes, until it's sent.
    changes text,
    sent integer not null default 0,
    fetched_at integer not null
) strict;
