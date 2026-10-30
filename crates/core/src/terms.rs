//! What a person agrees to before anything they add goes into the shared graph:
//! served by the graph, shown on a screen of its own, and kept with the version
//! each account agreed to. Changing a word changes the version, and everyone is
//! asked again.

/// The version of the terms, by the day they were written.
pub const VERSION: &str = "2026-10-30";

/// What the screen says, a point at a time, in plain words. Charging for
/// the service, and not being paid, are in the full terms, a link away.
pub const POINTS: [&str; 5] = [
    "What's added is knowledge: entries about ideas, how ideas relate, and the lessons your agent learns as it works, with whether the lessons it was given held. Never your code, file names, paths or notes.",
    "What you add is public domain, under CC0: anyone may read, copy, change and use it, for anything, without asking or naming anyone.",
    "Your agent writes it with your model. You confirm you may share it.",
    "It stays free for good: CC0 can’t be taken back, by kelve or anyone.",
    "It’s credited to a name the graph makes up for you, never your own name.",
];

/// The terms in full, as markdown: what the points say, as a licence.
pub fn text() -> String {
    format!(
        "# Adding to kelve's shared graph\n\nVersion {VERSION}.\n\n{}\n\n## What you add\n\nWhat your agent writes for the graph: entries about ideas, with their sources; how ideas relate; and lessons, what your agent learned as it worked, so the next agent doesn't have to learn it again. kelve holds each lesson a day before it goes, so you can stop it. What came of the lessons your agent was given goes too: whether each held, with nothing from your work. So do the reports you make about an entry.\n\n## The dedication you make\n\nYou dedicate what you add to the public domain under Creative Commons CC0 1.0 Universal (https://creativecommons.org/publicdomain/zero/1.0/). You give up copyright, database right and any similar rights in it, everywhere, as far as the law allows; where it doesn't, CC0's fallback licence applies. That includes anything you're treated as the author or owner of, such as a computer-generated work. You waive your moral rights in it too, as far as the law allows.\n\nkelve does the same for the graph as a whole: everything in it, the graph as a database included, is CC0.\n\n## What kelve may do\n\nBecause it's CC0, anyone may use what's in the graph, kelve included, and businesses and AI companies too. kelve may charge for the service around it, such as faster or bigger access, or access beyond the app. It can't charge for, or take back, the content itself. You won't be paid for what you add, now or later, even if kelve charges others for the service.\n\nIf you delete your account, what you added stays, under a made-up name that no longer leads to you.\n\n## What you promise\n\nYou may add what you add. Your agent wrote it on your own plan or key, within its terms. It holds nothing from your own work beyond what a general entry or lesson about an idea needs.\n\nWhat you add is credited to kelve's contributors, under the name the graph made up for you, though CC0 asks no one to credit anyone.\n\n## The rest\n\nThis is part of kelve's terms for the shared graph and kelve.ai, in full at https://kelve.ai/terms. How the shared graph handles what it keeps about you, and your rights, is at https://kelve.ai/privacy.\n",
        POINTS
            .iter()
            .map(|p| format!("- {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_terms_say_every_point() {
        let text = text();
        assert!(text.contains(VERSION));
        for point in POINTS {
            assert!(text.contains(point), "{point}");
        }
        assert!(text.contains("CC0 1.0 Universal"));
        assert!(text.contains("the graph as a database included, is CC0"));
        assert!(text.contains("You won't be paid"));
        assert!(text.contains("https://kelve.ai/terms"));
        assert!(text.contains("https://kelve.ai/privacy"));
    }
}
