# How ledger retries calls

Every call ledger makes to the bank can fail for reasons that pass: a
timeout, a dropped connection, a bank busy for a moment. Those calls are
tried again, up to six times, and a call that fails in a way a retry can't
fix fails at once.

Each wait is longer than the last. The first is about 200 milliseconds,
and each one after may be twice as long, up to 30 seconds. That gives a
struggling bank room to recover instead of more load.

The wait is never exact. Each client picks a random time below the
ceiling, so a hundred clients that failed together don't all come back at
the same moment and knock the bank over again.

## Why a retry can't charge twice

Every payment carries a key, made once by the caller and sent with every
try. The bank remembers the keys it has answered, so a retry of a payment
that already went through changes nothing. Without the key, a payment
that timed out but had in fact gone through would be taken again.

A call that hangs gives up after 15 seconds, so the retry around it can
start; see `src/bank/mod.rs`.
