-- Notes are gone: the person's own, those with the work, and those for their
-- agent. What's left of them is a debrief's Change it, which comes to the agent
-- as a job of its own (kind 'redo'), so what its hook told it of is by job. The
-- files people kept as notes stay where they are: kelve no longer reads them.

drop table if exists notes_told;

create table redos_told (
    agent text not null,
    job integer not null,
    at integer not null,
    primary key (agent, job)
) strict;

-- Jobs of a kind no kelve asks for any more.
delete from asks where kind = 'note';
delete from jobs where kind = 'note.done';
