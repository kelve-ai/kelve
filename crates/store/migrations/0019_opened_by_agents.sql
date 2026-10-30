-- What the person's agent opened: its own line about the work, which agent
-- it was, and, when it opened only some files of a folder, which, as JSON
-- (docs/specs/agent.md, section 5.1).
alter table opened add column said text not null default '';
alter table opened add column agent text not null default '';
alter table opened add column only text;
