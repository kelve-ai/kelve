-- Each request we've answered, by the key its caller sent, so a request
-- sent again gets the same answer instead of a second charge.
CREATE TABLE requests (
    key TEXT PRIMARY KEY,
    receipt TEXT NOT NULL,
    at TIMESTAMPTZ NOT NULL DEFAULT now()
);
