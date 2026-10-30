-- Plainly (docs/specs/plainly.md): a file, a folder and a change, each read
-- plainly by the person's agent, kept as checked. The person's own: none of
-- it goes to the shared graph. Kept by what it's about and what it was
-- written from, under the instructions' version:
--   a file:   at its text's hash, from the text it had before a change, or '';
--   a folder: at its project and path, from a hash of what it was given;
--   a change: at its project and which change, from a hash of its diff.
-- Code files read plainly before, a part at a time, are written afresh.
drop table kept_plainly;
create table plainly (
  kind text not null check (kind in ('file', 'folder', 'change')),
  at text not null,
  print text not null,
  version text not null,
  -- What's kept, as JSON: the view's own shape, checked.
  answer text not null,
  written_at integer not null,
  primary key (kind, at, print, version)
);
create index plainly_latest on plainly (kind, at, version, written_at);
