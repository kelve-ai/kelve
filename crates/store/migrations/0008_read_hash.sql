-- What a file read in a folder held when it was read, as a hash of its
-- text: reading a folder in the background reads again only the files that
-- have changed since, and their passages that changed are all it marks.
alter table read_files add column hash text;
