-- What the person chose for each folder's first read: every file, the
-- ones the rest lean on most, or only those they open; read directly, at
-- full price, rather than as batches. Until there's a row, kelve reads
-- nothing ahead in the folder.
create table first_reads (
    root text primary key,
    choice text not null check (choice in ('all', 'some', 'opened')),
    files integer,
    direct integer not null default 0,
    chosen_at integer not null
) strict;
