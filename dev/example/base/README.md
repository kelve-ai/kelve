# ledger

A payments service that keeps every movement of money as an entry in a
ledger, and never charges twice.

- Every charge, refund and fee is two entries: money leaves one account and
  arrives in another, so the books always balance (`src/ledger.rs`).
- A charge first reserves the money, then settles it once the bank says yes
  (`src/reserve.rs`).
- Calls to the bank are tried again when they fail for reasons that pass
  (`src/retry.rs`).
- The bank tells us what happened through signed webhooks
  (`src/api/v1/webhooks.rs`).

Run it with `cargo run`, against a Postgres named in `DATABASE_URL`.
