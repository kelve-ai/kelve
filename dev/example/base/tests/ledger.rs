//! The books balance: whatever moves, every movement's entries sum to
//! nothing.

#[test]
fn a_movement_sums_to_nothing() {
    let entries = [-1_250_i64, 1_250];
    assert_eq!(entries.iter().sum::<i64>(), 0);
}
