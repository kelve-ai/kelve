//! The breaker opens after enough failures in a row, stays open while it
//! cools off, then lets one call through to test the bank.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Closed { failures: u32 },
    Open { until: Instant },
    HalfOpen { trying: bool },
}

const TRIP_AFTER: u32 = 5;
const COOL_OFF: Duration = Duration::from_secs(30);

/// The breaker's rules, as `src/bank/breaker.rs` has them.
fn ask(state: State, now: Instant) -> (State, bool) {
    match state {
        State::Closed { .. } => (state, true),
        State::Open { until } if now < until => (state, false),
        State::Open { .. } | State::HalfOpen { trying: false } => (State::HalfOpen { trying: true }, true),
        State::HalfOpen { trying: true } => (state, false),
    }
}

fn failed(state: State, now: Instant) -> State {
    match state {
        State::Closed { failures } if failures + 1 >= TRIP_AFTER => State::Open { until: now + COOL_OFF },
        State::Closed { failures } => State::Closed { failures: failures + 1 },
        State::HalfOpen { .. } | State::Open { .. } => State::Open { until: now + COOL_OFF },
    }
}

#[test]
fn four_failures_leave_it_closed() {
    let now = Instant::now();
    let mut state = State::Closed { failures: 0 };
    for _ in 0..4 {
        state = failed(state, now);
    }
    assert_eq!(state, State::Closed { failures: 4 });
    assert!(ask(state, now).1);
}

#[test]
fn the_fifth_opens_it() {
    let now = Instant::now();
    let state = failed(State::Closed { failures: 4 }, now);
    assert_eq!(state, State::Open { until: now + COOL_OFF });
    assert!(!ask(state, now + Duration::from_secs(29)).1);
}

#[test]
fn once_cooled_off_one_call_tests_the_bank() {
    let now = Instant::now();
    let open = State::Open { until: now };
    let (testing, first) = ask(open, now + Duration::from_secs(1));
    assert!(first);
    assert_eq!(testing, State::HalfOpen { trying: true });
    // A second call while the test is going is turned away.
    assert!(!ask(testing, now + Duration::from_secs(1)).1);
}

#[test]
fn a_failed_test_opens_it_again() {
    let now = Instant::now();
    let state = failed(State::HalfOpen { trying: true }, now);
    assert_eq!(state, State::Open { until: now + COOL_OFF });
}
