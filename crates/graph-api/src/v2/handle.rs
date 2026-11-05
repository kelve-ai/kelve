//! A page's address, `@sam`: the rules anyone can check as it's typed,
//! with the sentence for each. Whether someone has it, whether it's kept
//! for kelve, and whether it's fit to show, only the service can say.

/// The fewest and most characters an address may have.
pub const FEWEST: usize = 2;
pub const MOST: usize = 24;

/// What the rules say as the person types, before anything is asked.
pub const RULES: &str = "Letters, numbers and hyphens, up to 24.";

/// An address as it's kept: trimmed, without a leading `@`, in lower case.
pub fn normal(typed: &str) -> String {
    typed.trim().trim_start_matches('@').to_lowercase()
}

/// The address as it's kept, or what to change, in a sentence.
pub fn check(typed: &str) -> Result<String, &'static str> {
    let h = normal(typed);
    if h.is_empty() {
        return Err("Choose an address.");
    }
    if !h
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("Use letters, numbers and hyphens.");
    }
    if h.starts_with('-') || h.ends_with('-') {
        return Err("Start and end with a letter or a number.");
    }
    if h.contains("--") {
        return Err("Use one hyphen at a time.");
    }
    if h.len() < FEWEST {
        return Err("Use at least 2 characters.");
    }
    if h.len() > MOST {
        return Err("Use 24 characters or fewer.");
    }
    Ok(h)
}

/// Others like `h` to offer when someone has it, longest last, each within
/// the rules.
pub fn instead(h: &str) -> Vec<String> {
    ["-agent", "builds", "-2", "-3"]
        .iter()
        .map(|end| format!("{h}{end}"))
        .filter(|c| check(c).is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_kept_trimmed_and_in_lower_case() {
        assert_eq!(check(" @Sam "), Ok("sam".into()));
        assert_eq!(check("sam-clark-2"), Ok("sam-clark-2".into()));
    }

    #[test]
    fn each_rule_says_what_to_change() {
        assert_eq!(check(""), Err("Choose an address."));
        assert_eq!(check("sam_c"), Err("Use letters, numbers and hyphens."));
        assert_eq!(check("sám"), Err("Use letters, numbers and hyphens."));
        assert_eq!(
            check("-sam"),
            Err("Start and end with a letter or a number.")
        );
        assert_eq!(
            check("sam-"),
            Err("Start and end with a letter or a number.")
        );
        assert_eq!(check("sa--m"), Err("Use one hyphen at a time."));
        assert_eq!(check("s"), Err("Use at least 2 characters."));
        assert_eq!(check(&"s".repeat(25)), Err("Use 24 characters or fewer."));
        assert!(check(&"s".repeat(24)).is_ok());
    }

    #[test]
    fn what_is_offered_instead_keeps_the_rules() {
        assert_eq!(instead("sam"), ["sam-agent", "sambuilds", "sam-2", "sam-3"]);
        assert!(instead(&"s".repeat(23)).is_empty());
    }
}
