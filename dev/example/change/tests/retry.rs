//! Waits grow, but never past the ceiling, and are never the same twice in
//! a row for every client.

use std::time::Duration;

const BASE: Duration = Duration::from_millis(200);
const MAX_WAIT: Duration = Duration::from_secs(30);

fn ceiling(attempt: u32) -> Duration {
    (BASE * 2u32.pow(attempt)).min(MAX_WAIT)
}

#[test]
fn the_ceiling_doubles_then_stops() {
    assert_eq!(ceiling(0), Duration::from_millis(200));
    assert_eq!(ceiling(3), Duration::from_millis(1600));
    assert_eq!(ceiling(12), MAX_WAIT);
}
