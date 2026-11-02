-- Each request we've answered, by the key its caller sent, so a request
-- sent again gets the same answer instead of a second charge.
CREATE TABLE requests (
    key TEXT PRIMARY KEY,
    -- A hash of the request's body: the same key with another body is a
    -- mistake, not a retry.
    fingerprint TEXT NOT NULL,
    -- 'started' while the request is going, 'answered' once it has been.
    state TEXT NOT NULL CHECK (state IN ('started', 'answered')),
    status INTEGER,
    body JSONB,
    at TIMESTAMPTZ NOT NULL DEFAULT now(),
    answered_at TIMESTAMPTZ,
    CHECK ((state = 'answered') = (status IS NOT NULL AND body IS NOT NULL))
);

-- Old answers are forgotten after a day.
CREATE INDEX requests_answered ON requests (answered_at) WHERE state = 'answered';
