-- Checks as the skills page starts them (docs/skills.md): the start each was
-- asked in, so the page says how far it's got, and how close each run came
-- to what the person kept, from 0 to 1.
alter table checks add column batch integer not null default 0;
alter table checks add column with_close real;
alter table checks add column without_close real;
create index checks_batch on checks (batch);

-- Fits you: each side's change as the agent handed it in, secrets masked,
-- to show side by side. None till it's handed in.
alter table fits_picks add column skills_diff text;
alter table fits_picks add column standard_diff text;
