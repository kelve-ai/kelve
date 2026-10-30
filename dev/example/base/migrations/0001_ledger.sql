-- Every movement of money is two rows, which sum to nothing.
CREATE TABLE entries (
    id BIGSERIAL PRIMARY KEY,
    movement UUID NOT NULL,
    account TEXT NOT NULL,
    amount BIGINT NOT NULL,
    currency CHAR(3) NOT NULL,
    why TEXT NOT NULL,
    at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX entries_by_account ON entries (account, currency);
