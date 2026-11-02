-- When the person's agent last edited each file it touched, apart from when
-- it last read it: as its turn ends, kelve's hook asks it to show work it
-- edited and hasn't shown in kelve since (docs/architecture.md).
alter table touched add column edited_at integer;
