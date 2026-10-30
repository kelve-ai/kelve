-- The person dismissed a hand-off's line under the note for agents: it
-- shows no more.
alter table handoffs add column dismissed_at integer;
