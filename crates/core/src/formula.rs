//! The formulas an explorable can use (`entry::Formula`). kelve computes
//! them; a model only picks one and sets its inputs. Each takes its inputs
//! by name, and gives `None` for inputs it can't work with, never a panic
//! or a number that isn't one.

use std::collections::BTreeMap;

use crate::entry::Formula;

/// The most trials `ChanceAtLeast` adds up. Past this a slider means
/// nothing to anyone, and the sum stays quick.
const MOST_TRIALS: f64 = 100_000.0;

/// The names of a formula's inputs, as an explorable's inputs must name
/// them, in the order they're usually shown.
pub fn names(formula: Formula) -> &'static [&'static str] {
    match formula {
        Formula::LoanPayment => &["principal", "rate", "years"],
        Formula::CompoundGrowth => &["start", "rate", "years"],
        Formula::ChanceAtLeast => &["k", "n", "p"],
        Formula::AgreementBeyondChance => &["observed", "byChance"],
        Formula::PercentChange => &["before", "after"],
        Formula::RateTimesTime => &["rate", "time"],
    }
}

/// The formula in words and symbols, for the exported markdown and for a
/// model choosing one.
pub fn written(formula: Formula) -> &'static str {
    match formula {
        Formula::LoanPayment => {
            "payment = principal × r ÷ (1 − (1 + r)^−m), where r is the yearly rate ÷ 12 and m is years × 12"
        }
        Formula::CompoundGrowth => "amount = start × (1 + rate)^years",
        Formula::ChanceAtLeast => {
            "chance = the sum, for every i from k to n, of C(n, i) × p^i × (1 − p)^(n − i)"
        }
        Formula::AgreementBeyondChance => "kappa = (observed − byChance) ÷ (1 − byChance)",
        Formula::PercentChange => "change = (after − before) ÷ before × 100",
        Formula::RateTimesTime => "amount = rate × time",
    }
}

/// The unit of a formula's result, when the formula decides it. The others
/// are in whatever the inputs are in.
pub fn unit(formula: Formula) -> Option<&'static str> {
    match formula {
        Formula::ChanceAtLeast | Formula::PercentChange => Some("%"),
        _ => None,
    }
}

/// Works out a formula from its inputs, by name. Missing inputs, inputs
/// that aren't finite, and inputs outside what the formula can take (a
/// rate of −100% or less, a chance outside 0 to 100, no months to pay a
/// loan over) give `None`.
pub fn compute(formula: Formula, inputs: &BTreeMap<String, f64>) -> Option<f64> {
    let get = |name: &str| inputs.get(name).copied().filter(|v| v.is_finite());
    let result = match formula {
        Formula::LoanPayment => loan_payment(get("principal")?, get("rate")?, get("years")?),
        Formula::CompoundGrowth => compound_growth(get("start")?, get("rate")?, get("years")?),
        Formula::ChanceAtLeast => chance_at_least(get("k")?, get("n")?, get("p")?),
        Formula::AgreementBeyondChance => kappa(get("observed")?, get("byChance")?),
        Formula::PercentChange => percent_change(get("before")?, get("after")?),
        Formula::RateTimesTime => Some(get("rate")? * get("time")?),
    }?;
    result.is_finite().then_some(result)
}

/// The monthly payment on a loan of `principal` at `rate`% a year over
/// `years`. At 0% it's the principal split evenly.
fn loan_payment(principal: f64, rate: f64, years: f64) -> Option<f64> {
    let months = (years * 12.0).round();
    if months < 1.0 || rate <= -1200.0 {
        return None;
    }
    let r = rate / 1200.0;
    if r.abs() < 1e-12 {
        return Some(principal / months);
    }
    // (1 + r)^-m, through logs so a long loan at a high rate doesn't
    // overflow on the way.
    let discount = (-months * r.ln_1p()).exp();
    let denominator = 1.0 - discount;
    if denominator.abs() < f64::EPSILON {
        return None;
    }
    Some(principal * r / denominator)
}

/// `start` grown at `rate`% a year for `years`.
fn compound_growth(start: f64, rate: f64, years: f64) -> Option<f64> {
    if rate <= -100.0 {
        return None;
    }
    Some(start * (years * (rate / 100.0).ln_1p()).exp())
}

/// The chance, in percent, that at least `k` of `n` go one way when each
/// does with chance `p`%. The binomial terms are summed in logs, so large
/// `n` neither overflows nor underflows to nothing.
fn chance_at_least(k: f64, n: f64, p: f64) -> Option<f64> {
    let (k, n) = (k.round(), n.round());
    if !(0.0..=MOST_TRIALS).contains(&n) || !(0.0..=100.0).contains(&p) {
        return None;
    }
    if k <= 0.0 {
        return Some(100.0);
    }
    if k > n {
        return Some(0.0);
    }
    let p = p / 100.0;
    if p <= 0.0 {
        return Some(0.0);
    }
    if p >= 1.0 {
        return Some(100.0);
    }
    // Both are whole numbers no bigger than MOST_TRIALS, so these casts are
    // exact.
    let (k, n) = (k as u32, n as u32);
    let (ln_p, ln_q) = (p.ln(), (-p).ln_1p());
    // ln C(n, i), built up from ln C(n, 0) = 0.
    let mut ln_choose = 0.0;
    let mut terms = Vec::with_capacity((n - k + 1) as usize);
    for i in 1..=n {
        ln_choose += f64::from(n - i + 1).ln() - f64::from(i).ln();
        if i >= k {
            terms.push(ln_choose + f64::from(i) * ln_p + f64::from(n - i) * ln_q);
        }
    }
    let top = terms.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !top.is_finite() {
        return Some(0.0);
    }
    let sum: f64 = terms.iter().map(|t| (t - top).exp()).sum();
    Some(((top + sum.ln()).exp() * 100.0).clamp(0.0, 100.0))
}

/// Cohen's kappa from `observed` agreement and agreement `byChance`, both
/// in percent: between −1 and 1. Undefined when chance alone agrees every
/// time.
fn kappa(observed: f64, by_chance: f64) -> Option<f64> {
    let observed = (observed / 100.0).clamp(0.0, 1.0);
    let by_chance = (by_chance / 100.0).clamp(0.0, 1.0);
    if by_chance >= 1.0 {
        return None;
    }
    Some(((observed - by_chance) / (1.0 - by_chance)).clamp(-1.0, 1.0))
}

/// The change from `before` to `after`, in percent of `before`. A fall from
/// a negative number is still a fall.
fn percent_change(before: f64, after: f64) -> Option<f64> {
    if before == 0.0 {
        return None;
    }
    Some((after - before) / before.abs() * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(formula: Formula, inputs: &[(&str, f64)]) -> Option<f64> {
        let inputs = inputs
            .iter()
            .map(|(k, v)| ((*k).to_string(), *v))
            .collect::<BTreeMap<_, _>>();
        compute(formula, &inputs)
    }

    fn close(got: Option<f64>, want: f64, within: f64) {
        let got = got.unwrap_or(f64::NAN);
        assert!((got - want).abs() <= within, "got {got}, want {want}");
    }

    #[test]
    fn a_loan_pays_what_the_tables_say() {
        // £200,000 over 25 years at 5% is £1,169.18 a month.
        let pay = run(
            Formula::LoanPayment,
            &[("principal", 200_000.0), ("rate", 5.0), ("years", 25.0)],
        );
        close(pay, 1169.18, 0.01);
        // $100,000 over 30 years at 6% is $599.55.
        let pay = run(
            Formula::LoanPayment,
            &[("principal", 100_000.0), ("rate", 6.0), ("years", 30.0)],
        );
        close(pay, 599.55, 0.01);
    }

    #[test]
    fn a_loan_at_no_interest_is_split_evenly() {
        let pay = run(
            Formula::LoanPayment,
            &[("principal", 12_000.0), ("rate", 0.0), ("years", 1.0)],
        );
        close(pay, 1000.0, 1e-9);
        // Close to zero behaves like zero, not like a division by nothing.
        let pay = run(
            Formula::LoanPayment,
            &[("principal", 12_000.0), ("rate", 1e-15), ("years", 1.0)],
        );
        close(pay, 1000.0, 1e-6);
    }

    #[test]
    fn a_loan_over_no_time_has_no_payment() {
        assert_eq!(
            run(
                Formula::LoanPayment,
                &[("principal", 1000.0), ("rate", 5.0), ("years", 0.0)],
            ),
            None
        );
        assert_eq!(
            run(
                Formula::LoanPayment,
                &[("principal", 1000.0), ("rate", -1300.0), ("years", 1.0)],
            ),
            None
        );
    }

    #[test]
    fn growth_compounds() {
        close(
            run(
                Formula::CompoundGrowth,
                &[("start", 1000.0), ("rate", 7.0), ("years", 10.0)],
            ),
            1967.15,
            0.01,
        );
        close(
            run(
                Formula::CompoundGrowth,
                &[("start", 1000.0), ("rate", 0.0), ("years", 10.0)],
            ),
            1000.0,
            1e-9,
        );
        assert_eq!(
            run(
                Formula::CompoundGrowth,
                &[("start", 1000.0), ("rate", -100.0), ("years", 1.0)],
            ),
            None
        );
    }

    #[test]
    fn chance_at_least_matches_a_coin() {
        // Seven of seven heads: 1 in 128.
        close(
            run(
                Formula::ChanceAtLeast,
                &[("k", 7.0), ("n", 7.0), ("p", 50.0)],
            ),
            100.0 / 128.0,
            1e-9,
        );
        // Five or more of seven: 29 in 128, about one in four.
        close(
            run(
                Formula::ChanceAtLeast,
                &[("k", 5.0), ("n", 7.0), ("p", 50.0)],
            ),
            100.0 * 29.0 / 128.0,
            1e-9,
        );
        // At least one six in four rolls: 1 − (5/6)^4.
        close(
            run(
                Formula::ChanceAtLeast,
                &[("k", 1.0), ("n", 4.0), ("p", 100.0 / 6.0)],
            ),
            100.0 * (1.0 - (5.0f64 / 6.0).powi(4)),
            1e-9,
        );
    }

    #[test]
    fn chance_at_least_holds_up_at_the_edges() {
        let chance = |k, n, p| run(Formula::ChanceAtLeast, &[("k", k), ("n", n), ("p", p)]);
        assert_eq!(chance(0.0, 10.0, 30.0), Some(100.0));
        assert_eq!(chance(11.0, 10.0, 30.0), Some(0.0));
        assert_eq!(chance(3.0, 10.0, 0.0), Some(0.0));
        assert_eq!(chance(3.0, 10.0, 100.0), Some(100.0));
        assert_eq!(chance(3.0, 10.0, 101.0), None);
        assert_eq!(chance(3.0, -1.0, 50.0), None);
        assert_eq!(chance(3.0, 1e9, 50.0), None);
        // A large n neither overflows nor rounds to nothing: half or more
        // of 10,000 fair coins is a little over 50%.
        close(chance(5000.0, 10_000.0, 50.0), 50.4, 0.01);
        // And a tiny tail stays a tiny number, not NaN.
        let tail = chance(9000.0, 10_000.0, 50.0).unwrap_or(f64::NAN);
        assert!((0.0..1e-100).contains(&tail), "{tail}");
    }

    #[test]
    fn kappa_allows_for_chance() {
        // 76% agreement where chance gives 50% is kappa 0.52.
        close(
            run(
                Formula::AgreementBeyondChance,
                &[("observed", 76.0), ("byChance", 50.0)],
            ),
            0.52,
            1e-9,
        );
        close(
            run(
                Formula::AgreementBeyondChance,
                &[("observed", 120.0), ("byChance", 50.0)],
            ),
            1.0,
            1e-9,
        );
        close(
            run(
                Formula::AgreementBeyondChance,
                &[("observed", 0.0), ("byChance", 90.0)],
            ),
            -1.0,
            1e-9,
        );
        assert_eq!(
            run(
                Formula::AgreementBeyondChance,
                &[("observed", 100.0), ("byChance", 100.0)],
            ),
            None
        );
    }

    #[test]
    fn percent_change_and_rate_times_time() {
        close(
            run(
                Formula::PercentChange,
                &[("before", 80.0), ("after", 100.0)],
            ),
            25.0,
            1e-9,
        );
        close(
            run(
                Formula::PercentChange,
                &[("before", -50.0), ("after", -75.0)],
            ),
            -50.0,
            1e-9,
        );
        assert_eq!(
            run(Formula::PercentChange, &[("before", 0.0), ("after", 5.0)]),
            None
        );
        close(
            run(Formula::RateTimesTime, &[("rate", 60.0), ("time", 2.5)]),
            150.0,
            1e-9,
        );
    }

    #[test]
    fn missing_or_broken_inputs_give_nothing() {
        assert_eq!(run(Formula::RateTimesTime, &[("rate", 60.0)]), None);
        assert_eq!(
            run(Formula::RateTimesTime, &[("rate", f64::NAN), ("time", 1.0)]),
            None
        );
        assert_eq!(
            run(
                Formula::RateTimesTime,
                &[("rate", f64::INFINITY), ("time", 1.0)]
            ),
            None
        );
        assert_eq!(
            run(
                Formula::RateTimesTime,
                &[("rate", f64::MAX), ("time", 10.0)]
            ),
            None
        );
    }

    #[test]
    fn every_formula_names_its_inputs() {
        for formula in [
            Formula::LoanPayment,
            Formula::CompoundGrowth,
            Formula::ChanceAtLeast,
            Formula::AgreementBeyondChance,
            Formula::PercentChange,
            Formula::RateTimesTime,
        ] {
            let names = names(formula);
            assert!(!names.is_empty());
            assert!(!written(formula).is_empty());
            // With every input at a middling value, each one computes.
            let inputs: Vec<(&str, f64)> = names.iter().map(|n| (*n, 5.0)).collect();
            assert!(run(formula, &inputs).is_some(), "{formula:?}");
        }
    }
}
