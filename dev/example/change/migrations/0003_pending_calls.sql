-- Charges written down before the bank is asked, and taken off once it
-- answers. One still here a few minutes later is hanging: the reconciler
-- asks the bank what became of it.
CREATE TABLE pending_calls (
    key TEXT PRIMARY KEY REFERENCES requests (key),
    held UUID NOT NULL,
    customer TEXT NOT NULL,
    merchant TEXT NOT NULL,
    amount BIGINT NOT NULL CHECK (amount > 0),
    currency CHAR(3) NOT NULL,
    at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX pending_calls_by_age ON pending_calls (at);

-- Each event the bank has told us about, by its own ID, so an event the
-- bank sends twice is acted on once.
CREATE TABLE bank_events (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    request TEXT,
    at TIMESTAMPTZ NOT NULL DEFAULT now()
);
