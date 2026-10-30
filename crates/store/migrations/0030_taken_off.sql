-- A job the person took off their agent's list (docs/specs/reader.md,
-- 3.14) is put aside like one answered wrongly too often, but stays put
-- aside: asked for again by the work, or by opening the same thing again,
-- it isn't put back; only the person puts it back.
-- When they took it off; null for every other job.
alter table asks add column taken_off_at integer;
