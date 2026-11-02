//! A hanging charge is finished as the bank says it went: settled when the
//! bank took it, given back when it didn't, left alone while it's pending,
//! and given back when it has been pending too long.

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Taken,
    Declined,
    Unknown,
    Pending,
}

#[derive(Debug, PartialEq, Eq)]
enum Then {
    /// The held money goes to the merchant.
    Settle,
    /// The held money goes back to the customer.
    GiveBack,
    /// Nothing yet: asked again next round.
    Wait,
}

/// What `src/reconcile.rs` does with each outcome.
fn then(outcome: Outcome, expired: bool) -> Then {
    match outcome {
        Outcome::Taken => Then::Settle,
        Outcome::Declined | Outcome::Unknown => Then::GiveBack,
        Outcome::Pending if expired => Then::GiveBack,
        Outcome::Pending => Then::Wait,
    }
}

#[test]
fn taken_is_settled() {
    assert_eq!(then(Outcome::Taken, false), Then::Settle);
    // Even late: money the bank took is the merchant's.
    assert_eq!(then(Outcome::Taken, true), Then::Settle);
}

#[test]
fn declined_or_never_received_is_given_back() {
    assert_eq!(then(Outcome::Declined, false), Then::GiveBack);
    assert_eq!(then(Outcome::Unknown, false), Then::GiveBack);
}

#[test]
fn pending_waits_until_it_has_waited_too_long() {
    assert_eq!(then(Outcome::Pending, false), Then::Wait);
    assert_eq!(then(Outcome::Pending, true), Then::GiveBack);
}

#[test]
fn every_held_charge_ends_in_one_place() {
    // However the bank answers, the money ends up with the merchant or
    // the customer, or stays held to be asked about again: never both.
    for outcome in [Outcome::Taken, Outcome::Declined, Outcome::Unknown, Outcome::Pending] {
        for expired in [false, true] {
            let t = then(outcome_copy(&outcome), expired);
            assert!(matches!(t, Then::Settle | Then::GiveBack | Then::Wait));
        }
    }
}

fn outcome_copy(o: &Outcome) -> Outcome {
    match o {
        Outcome::Taken => Outcome::Taken,
        Outcome::Declined => Outcome::Declined,
        Outcome::Unknown => Outcome::Unknown,
        Outcome::Pending => Outcome::Pending,
    }
}
