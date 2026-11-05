-- kelve's v2 clear-out (docs/skills.md, "What we keep from kelve today, and
-- what we don't"): reading what's opened, the way in, reading plainly, the
-- shared graph's ideas, entries and relations, and lessons are gone, and
-- what they kept goes with them. What the hook sees as the agent works
-- (moments, touched, held_for_agent) stays.

drop table if exists marks;
drop table if exists read_files;
drop table if exists built_on;
drop table if exists plainly;
drop table if exists kept_way;
drop table if exists asked;
drop table if exists ideas_read;
drop table if exists follows;
drop table if exists sends;
drop table if exists graph_held;
drop table if exists idea_relations;
drop table if exists entries;
drop table if exists ideas;
drop table if exists given;
drop table if exists lesson_ideas;
drop table if exists lessons;
drop table if exists lessons_fetched;
drop table if exists learned;
drop table if exists failed;
drop table if exists written;
drop table if exists replaced;
drop table if exists files_seen;
drop table if exists uses;
drop table if exists libraries;
drop table if exists walked_files;
drop table if exists walked_roots;
drop table if exists opened_in;

alter table opened drop column way;
alter table opened drop column steps_read;
alter table opened drop column woken_at;

-- Jobs of kinds no kelve asks for any more.
delete from asks where kind in ('read', 'choose', 'builtOn', 'relate', 'write', 'fix', 'extend',
    'folders', 'way', 'path', 'plainly', 'plainlyFolder', 'plainlyChange', 'lesson');
delete from jobs where kind like 'read.%' or kind like 'plainly.%' or kind like 'way.%'
    or kind like 'entry.%' or kind like 'lesson.%';

-- The shared graph's key, and until when sending to it waited.
delete from secrets where name = 'keys.graph';
