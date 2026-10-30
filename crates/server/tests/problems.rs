//! When kelve panics, it keeps a short report, tidied, and sends it
//! nowhere. Its own test binary: the panic hook is the whole process's.

#![allow(clippy::unwrap_used, clippy::panic)]

#[test]
fn a_panic_is_kept_as_a_tidy_report() {
    let dir = std::env::temp_dir().join(format!("kelve-panic-{}", std::process::id()));
    kelve_server::problems::watch(&dir);
    let caught = std::panic::catch_unwind(|| {
        panic!(
            "couldn't read /Users/sam/plans/secret-launch.md with key sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGH"
        );
    });
    assert!(caught.is_err());
    let kept = kelve_server::problems::list(&dir);
    assert_eq!(kept.len(), 1);
    let report = &kept[0];
    assert_eq!(report.what, "kelve stopped");
    assert!(
        report.text.contains("couldn't read [path]"),
        "{}",
        report.text
    );
    assert!(
        report.text.contains("crates/server/tests/problems.rs"),
        "where in kelve"
    );
    for private in ["sam", "secret-launch", "abcdefghij", "/Users/"] {
        assert!(
            !report.text.contains(private),
            "{private} in {}",
            report.text
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
