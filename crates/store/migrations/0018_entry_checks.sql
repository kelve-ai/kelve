-- How the shared graph's check of an entry the person's agent wrote
-- stands, claim by claim, as the graph API's Checking, as JSON: what the
-- reader shows while the entry is read before anyone else can.
alter table entries add column checking text;
