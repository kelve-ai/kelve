-- The steps of a way in the person has read, with what was opened, as a
-- JSON list of paths: In order fills each one's square (docs/specs/reader.md,
-- 3.11). Kept here, so a new store starts with none read.
alter table opened add column steps_read text;
