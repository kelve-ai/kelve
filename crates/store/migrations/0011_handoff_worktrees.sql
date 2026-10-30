-- In a git repository, each hand-off's agent works in a worktree of its
-- own, on a branch of its own (`server::worktree`): the folder it works
-- in, so `kelve done` from there finds it. What it did is brought into
-- the project when the person says it looks right, or thrown away.
alter table handoffs add column worktree text;
alter table handoffs add column dropped_at integer;
