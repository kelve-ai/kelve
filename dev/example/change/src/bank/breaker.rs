//! A circuit breaker for the bank: after enough calls in a row fail, stop
//! calling for a while, then let one call through to see if it's back.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::error::Error;

/// Where the breaker is. Closed lets every call through; open lets none
/// through until `until`; half open lets one through to test the bank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Closed { failures: u32 },
    Open { until: Instant },
    HalfOpen { trying: bool },
}

pub struct Breaker {
    state: Mutex<State>,
    /// Failures in a row that open the breaker.
    trip_after: u32,
    /// How long it stays open before it lets one call through.
    cool_off: Duration,
}

/// What the caller must do with the breaker's answer.
#[derive(Debug, PartialEq, Eq)]
pub enum Permit {
    /// Go ahead, then report how it went.
    Go,
    /// Go ahead: this call tests whether the bank is back.
    Test,
}

impl Breaker {
    pub fn new(config: &Config) -> Breaker {
        Breaker::with(config.breaker_trip_after, config.breaker_cool_off)
    }

    pub fn with(trip_after: u32, cool_off: Duration) -> Breaker {
        Breaker { state: Mutex::new(State::Closed { failures: 0 }), trip_after, cool_off }
    }

    /// Asks to make a call. An open breaker says no at once, so a bank
    /// that's down isn't asked again and again while it's down.
    pub fn ask(&self, now: Instant) -> Result<Permit, Error> {
        let mut state = self.state.lock().expect("the breaker's lock");
        match *state {
            State::Closed { .. } => Ok(Permit::Go),
            State::Open { until } if now < until => Err(Error::BankDown { retry_in: until - now }),
            State::Open { .. } => {
                *state = State::HalfOpen { trying: true };
                Ok(Permit::Test)
            }
            // One test call at a time; the rest wait as if it were still open.
            State::HalfOpen { trying: true } => Err(Error::BankDown { retry_in: self.cool_off }),
            State::HalfOpen { trying: false } => {
                *state = State::HalfOpen { trying: true };
                Ok(Permit::Test)
            }
        }
    }

    /// A call worked: the bank is up, so the breaker closes.
    pub fn worked(&self) {
        *self.state.lock().expect("the breaker's lock") = State::Closed { failures: 0 };
    }

    /// A call failed. Only failures that say the bank itself is in trouble
    /// count: a declined card is the bank working as it should.
    pub fn failed(&self, error: &Error, now: Instant) {
        if !counts(error) {
            self.release();
            return;
        }
        let mut state = self.state.lock().expect("the breaker's lock");
        *state = match *state {
            State::Closed { failures } if failures + 1 >= self.trip_after => State::Open { until: now + self.cool_off },
            State::Closed { failures } => State::Closed { failures: failures + 1 },
            // The test call failed: the bank is still down, so wait again.
            State::HalfOpen { .. } | State::Open { .. } => State::Open { until: now + self.cool_off },
        };
    }

    /// A test call ended without saying anything about the bank: let the
    /// next one test it instead.
    fn release(&self) {
        let mut state = self.state.lock().expect("the breaker's lock");
        if let State::HalfOpen { .. } = *state {
            *state = State::HalfOpen { trying: false };
        }
    }

    pub fn state(&self) -> State {
        *self.state.lock().expect("the breaker's lock")
    }
}

/// Whether a failure says the bank is in trouble: it timed out, couldn't
/// be reached, or failed on its side.
fn counts(error: &Error) -> bool {
    match error {
        Error::Bank(e) => e.is_timeout() || e.is_connect() || e.status().is_some_and(|s| s.is_server_error()),
        Error::Timeout => true,
        _ => false,
    }
}
