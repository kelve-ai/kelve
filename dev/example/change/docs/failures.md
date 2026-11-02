# When the bank fails

Retries handle a bank that fails for a moment; see `docs/retries.md`. This
page is about what happens when it fails for longer, or when we never hear
back at all.

## A bank that's down

After five calls in a row fail with a timeout, a dropped connection or an
error on the bank's side, ledger stops calling it for 30 seconds. Shops
get a 503 with a `Retry-After` header straight away, instead of waiting
for calls that won't work. A declined card doesn't count: that's the bank
working as it should.

After the 30 seconds, one call is let through to test the bank. If it
works, calls go through again. If it fails, ledger waits another 30
seconds. Both numbers are settings: `BREAKER_TRIP_AFTER` and
`BREAKER_COOL_OFF_SECONDS`.

## Not calling the bank too fast

Ledger calls the bank at most 20 times a second over time, with bursts of
up to 40. A call over the limit waits its turn rather than failing. When
many charges fail together and retry, this is what stops their retries
reaching the bank all at once. Set it with `BANK_CALLS_PER_SECOND` and
`BANK_BURST`, below what the bank's contract allows.

## A request sent twice

Each charge and refund carries a key. The key is claimed before anything
else happens, so two copies arriving together can't both reach the bank:
the second is told the first is still going, with a 409, and to ask again.
Once the first is answered, every copy gets the same answer, for a day.

The same key sent with a different request is a mistake, not a retry, and
is turned away with a 422.

## A charge we never heard back about

Sometimes a call times out after the bank took the money, or ledger stops
while it waits. The money stays held and the charge stays pending. Every
minute, the reconciler asks the bank what became of each charge pending
for more than three minutes:

- **Taken:** the money goes to the merchant.
- **Declined, or never received:** it goes back to the customer.
- **Still pending:** it's asked again next time, for up to a day. After
  that, the money goes back to the customer, and the log says so.

Only one copy of ledger reconciles at a time, so a charge is never finished
twice. The bank's webhooks can finish a charge sooner: each event is acted
on once, by its ID, however many times the bank sends it.
