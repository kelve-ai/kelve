-- A way into what's opened, as the person's agent gave it: its files in an
-- order, as JSON (docs/specs/reader.md, 3.11). And, for a project whose
-- folder lines and "why"s can't be kept in it (outside git, or kept to
-- kelve in settings), kelve keeps them here: the project's way, and each
-- "why" by what it's about, as JSON.
alter table opened add column way text;

create table kept_way (
  root text primary key,
  way text not null
);

create table kept_why (
  root text not null,
  at text not null,
  why text not null,
  primary key (root, at)
);
