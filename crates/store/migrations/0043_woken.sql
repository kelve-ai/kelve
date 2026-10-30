-- When the person's agent was last asked to do what's waiting for one thing
-- opened, by its number (docs/specs/reader.md, 3.14): its reading isn't
-- paused for something opened after it while that agent works on it, so
-- agents in several tabs each carry on with their own.
alter table opened add column woken_at integer;
