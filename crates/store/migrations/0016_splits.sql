-- An idea the shared graph found to be two: the two it is now, as JSON
-- ([{"id", "name"}]), for the reader to offer instead.
alter table ideas add column split_into text;
