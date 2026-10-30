-- Code files read plainly (docs/specs/reader.md, 3.16): each part as a
-- sentence on what it does, as the person's agent wrote them. Kept by a
-- hash of the text the agent was given, and the instructions it was
-- written under, so it's written once for each text and reused wherever
-- that text is opened. The person's own: it never goes to the shared graph.
create table kept_plainly (
  text_hash text not null,
  version text not null,
  -- The parts, as JSON: `[{from, to, says}]`.
  parts text not null,
  written_at integer not null,
  primary key (text_hash, version)
);
