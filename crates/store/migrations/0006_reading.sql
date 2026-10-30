-- Reading a folder in the background (spec 6.2).

-- Why a file found in a folder couldn't be read, when reading it found
-- out: it isn't text, say. Most rows have none.
alter table read_files add column skip text;

-- Which folder a choice of what it's built on was for, and how many of its
-- files had been read: while a folder is read in the background, it's
-- chosen again each time the files read double, not after every one.
alter table built_on add column root text not null default '';
alter table built_on add column files integer not null default 0;
