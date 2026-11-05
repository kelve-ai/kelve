-- The text each part's check was last started for, as its hash. When checks
-- run by themselves, kelve starts a part's check once for each text it has:
-- one the person stopped, or one that couldn't be done, waits for them.
alter table parts add column tried text;
