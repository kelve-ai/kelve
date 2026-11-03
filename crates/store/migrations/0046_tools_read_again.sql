-- The tools the person's agent ran, kept under no file (`server::lessons`),
-- were read from every line of a command, quoted text and heredocs too, so
-- words from commit messages and scripts were kept as tools, and offered as
-- what a lesson could be about. They're read as the shell does now: these
-- are let go, and kept again as the agent runs commands.
delete from uses where path = '';
