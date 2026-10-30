-- When a folder's first read, as chosen, was done: until then, coming
-- back to the folder shows how far it's got.
alter table first_reads add column done_at integer;
