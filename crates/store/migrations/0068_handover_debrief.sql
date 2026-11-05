-- Which debrief covers a handover. More on the same work moves a debrief on
-- to the newest handover, so the earlier ones are covered by it too: kept
-- here, not worked out from times, which two rows can share.
alter table handovers add column debrief integer references debriefs (id) on delete set null;

update handovers set debrief = (
  select d.id from debriefs d where d.handover = handovers.id order by d.made_at desc limit 1
);
