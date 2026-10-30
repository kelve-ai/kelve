-- Where a job goes among those of its priority: the id of the job it
-- finishes, so checking a file's ideas comes straight after reading it,
-- ahead of what was asked for since; or, with none, its own id.
alter table asks add column place integer;
