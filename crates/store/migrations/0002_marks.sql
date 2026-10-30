-- Each passage's marks, kept against the passage's hash, the version of
-- what kelve asks and the model that answered: an unchanged passage is
-- never marked twice, and an edit re-marks only what changed. Lines are
-- counted from the passage's first, so a passage that moves keeps its
-- marks.
create table marks (
    passage text not null,
    prompt text not null,
    model text not null,
    marks text not null,
    made_at integer not null,
    primary key (passage, prompt, model)
) strict;
