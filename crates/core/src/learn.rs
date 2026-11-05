//! Learning from the person (docs/skills.md): which of their messages
//! correct their agent, where something new goes among their skills (the
//! three questions), the answer the `learn` job hands in, read and checked,
//! and the skill drafted for a kind of work when the person has none yet.
//! Every problem is a sentence the agent can act on.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::skills::{self, SectionKind, Skill, WorkKind, work::one_kind};
use crate::text::re;

/// The longest message read for a correction, in characters: a long one is
/// a request, not a correction.
pub const MOST_CORRECTION: usize = 600;
/// The longest heading a part has, in characters.
pub const MOST_HEADING: usize = 120;
/// The longest body a part has, in characters.
pub const MOST_BODY: usize = 1_500;
/// The most parts one answer adds.
pub const MOST_ADDITIONS: usize = 8;
/// How many separate pieces of work a task comes up in before its steps get
/// a skill of their own.
pub const TWICE: usize = 2;

static CORRECTS: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?ix)
        ^\s*(?:no|nope|not\s+like\s+that|wrong|stop|undo|revert|actually|wait|hmm|instead|rather)\b
        | \b(?:don'?t|do\s+not|never|always|should(?:n'?t)?|shouldn't|instead\s+of|rather\s+than|prefer|we\s+use|use\s+\S+\s+(?:here|instead)|that'?s\s+(?:not|wrong)|isn'?t\s+(?:how|what)|put\s+it\s+back|change\s+it\s+to|why\s+did\s+you|too\s+(?:long|short|big|small|many|much|clever|complicated))\b
        ")
});

/// Whether a message the person sent their agent while it worked corrects
/// it ("use a loop here", "that should exit 1"), rather than asking for
/// something new. A first guess, read from its words alone: the learn job
/// decides whether there's anything to keep, and none is a good answer.
pub fn corrects(text: &str) -> bool {
    let t = text.trim().replace('’', "'");
    !t.is_empty() && t.chars().count() <= MOST_CORRECTION && CORRECTS.is_match(&t)
}

/// Where something new goes among the person's skills.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    /// True only in this repository: the project's skill.
    Project,
    /// Steps for a task that has come up in at least two separate pieces of
    /// work: the "how you do something" skill for it, by its name.
    Do(String),
    /// A default: the "how you like it" skill for the kind of work it was
    /// caught in, or Everywhere when it was caught in more than one.
    Like(WorkKind),
}

/// kelve's three questions, in order. Is it true only in this repository?
/// The project's skill. Is it steps for one task that has come up in at
/// least two separate pieces of work? That task's skill. Otherwise it's a
/// default, for the kind of work it was caught in.
pub fn target(only_here: bool, task: Option<(&str, usize)>, kinds: &[WorkKind]) -> Target {
    if only_here {
        return Target::Project;
    }
    if let Some((name, times)) = task
        && times >= TWICE
        && skills::name_problem(name).is_none()
    {
        return Target::Do(name.to_string());
    }
    Target::Like(one_kind(kinds))
}

/// The "how you like it" skill kelve drafts for a kind of work the person
/// has none for yet: its front matter, title and what it's for, no parts.
pub fn like_draft(kind: WorkKind) -> String {
    let description = format!(
        "How the person you're working with likes {} done. {}",
        match kind {
            WorkKind::Everywhere => "any work".to_string(),
            WorkKind::Words => "the words people see in an app".to_string(),
            WorkKind::Docs => "docs".to_string(),
            WorkKind::Config => "settings, manifests and CI".to_string(),
            other => other.label().to_string(),
        },
        kind.when()
    );
    let intro = match kind {
        WorkKind::Everywhere => "What holds in every kind of work.".to_string(),
        other => format!("Defaults for {}, in any project.", other.label()),
    };
    Skill::new(kind.name(), &description, kind.label(), &intro).write()
}

/// The project's skill, as kelve names it for a repository: its folder's
/// name, made into a skill's name, or `project`.
pub fn project_name(root: &str) -> String {
    let last = root
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    let mut name = String::new();
    for c in last.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            name.push(c);
        } else if !name.ends_with('-') && !name.is_empty() {
            name.push('-');
        }
    }
    let name: String = name
        .trim_matches('-')
        .chars()
        .take(skills::MOST_NAME)
        .collect();
    let name = name.trim_end_matches('-').to_string();
    if skills::name_problem(&name).is_none() {
        name
    } else {
        "project".into()
    }
}

/// A part to add to a skill.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Addition {
    pub skill: String,
    pub section: SectionKind,
    /// The part it goes after; none for the end of its section.
    pub after: Option<String>,
    pub heading: String,
    pub body: String,
    pub evidence: String,
}

/// Where what was shown is already a part.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Same {
    pub skill: String,
    pub part: String,
    pub evidence: String,
}

/// Where what was shown contradicts a part, and the part that would replace
/// it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disagrees {
    pub skill: String,
    pub part: String,
    pub heading: String,
    pub body: String,
    pub evidence: String,
}

/// A new skill to suggest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewSkill {
    pub name: String,
    pub description: String,
    /// `do` or `project`.
    pub kind: skills::SkillKind,
    pub markdown: String,
}

/// The learn job's answer, read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Learned {
    pub additions: Vec<Addition>,
    pub same: Vec<Same>,
    pub disagrees: Vec<Disagrees>,
    pub new_skill: Option<NewSkill>,
}

impl Learned {
    pub fn is_empty(&self) -> bool {
        self.additions.is_empty()
            && self.same.is_empty()
            && self.disagrees.is_empty()
            && self.new_skill.is_none()
    }
}

/// The shape the learn job's answer must fit, as a JSON Schema.
pub fn schema() -> Value {
    let part = json!({
        "type": "object",
        "properties": {
            "skill": { "type": "string" },
            "section": { "type": "string", "enum": ["howYouLikeIt", "steps", "watchOutFor"] },
            "after": { "type": "string" },
            "heading": { "type": "string" },
            "body": { "type": "string" },
            "evidence": { "type": "string" },
        },
        "required": ["skill", "section", "heading", "body", "evidence"],
    });
    json!({
        "type": "object",
        "properties": {
            "additions": { "type": "array", "items": part },
            "same": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "skill": { "type": "string" },
                        "part": { "type": "string" },
                        "evidence": { "type": "string" },
                    },
                    "required": ["skill", "part", "evidence"],
                },
            },
            "disagrees": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "skill": { "type": "string" },
                        "part": { "type": "string" },
                        "heading": { "type": "string" },
                        "body": { "type": "string" },
                        "evidence": { "type": "string" },
                    },
                    "required": ["skill", "part", "heading", "body", "evidence"],
                },
            },
            "newSkill": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "description": { "type": "string" },
                    "kind": { "type": "string", "enum": ["do", "project"] },
                    "markdown": { "type": "string" },
                },
                "required": ["name", "description", "kind", "markdown"],
            },
        },
        "required": ["additions", "same", "disagrees"],
    })
}

/// A skill the learn job was given, whole, by its name and text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Given<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

fn text(v: &Value, field: &str) -> String {
    v[field].as_str().unwrap_or_default().trim().to_string()
}

fn section(word: &str) -> Option<SectionKind> {
    match word {
        "howYouLikeIt" => Some(SectionKind::HowYouLikeIt),
        "steps" => Some(SectionKind::Steps),
        "watchOutFor" => Some(SectionKind::WatchOutFor),
        _ => None,
    }
}

/// A part's heading and body checked: what's wrong, as a sentence.
fn part_problem(what: &str, heading: &str, body: &str) -> Option<String> {
    if heading.is_empty() {
        return Some(format!(
            "{what} has no heading: give the rule itself, in a few words."
        ));
    }
    if heading.chars().count() > MOST_HEADING {
        return Some(format!(
            "{what}'s heading is longer than {MOST_HEADING} characters: say the rule in a few words, and why in its body."
        ));
    }
    if heading.contains('\n') {
        return Some(format!("{what}'s heading is more than one line."));
    }
    if body.is_empty() {
        return Some(format!(
            "{what} (“{heading}”) has no body: say why, in a sentence or two."
        ));
    }
    if body.chars().count() > MOST_BODY {
        return Some(format!(
            "{what} (“{heading}”) is longer than {MOST_BODY} characters: keep the why to a sentence or two, and an example only where it's needed."
        ));
    }
    if crate::md::lines(body)
        .iter()
        .any(|l| !l.code && crate::md::heading(l.text).is_some())
    {
        return Some(format!(
            "{what} (“{heading}”) has a heading in its body, which would end the part: use bold, or a list."
        ));
    }
    if crate::redact::redact(body) != body || crate::redact::redact(heading) != heading {
        return Some(format!(
            "{what} (“{heading}”) seems to hold a secret. Take it out."
        ));
    }
    None
}

/// Reads the learn job's answer, against the skills it was given whole:
/// what to add and where, or every problem, as sentences the agent can act
/// on. Every skill named must be one given, and every part named must be in
/// it, as written.
pub fn read(value: &Value, given: &[Given<'_>]) -> Result<Learned, Vec<String>> {
    let mut problems = Vec::new();
    if !value.is_object() {
        return Err(vec![
            "The answer must be one JSON object, with additions, same and disagrees.".into(),
        ]);
    }
    let names: Vec<&str> = given.iter().map(|g| g.name).collect();
    let skill_of = |name: &str, problems: &mut Vec<String>| -> Option<Skill> {
        match given.iter().find(|g| g.name == name) {
            Some(g) => Some(Skill::parse(g.text)),
            None => {
                problems.push(format!(
                    "There's no skill “{name}” among those given. Use one of: {}.",
                    names.join(", ")
                ));
                None
            }
        }
    };
    let list = |field: &str, problems: &mut Vec<String>| -> Vec<Value> {
        match &value[field] {
            Value::Null => Vec::new(),
            Value::Array(items) => items.clone(),
            _ => {
                problems.push(format!("{field} must be a list: empty when there's none."));
                Vec::new()
            }
        }
    };
    let mut learned = Learned::default();
    let additions = list("additions", &mut problems);
    if additions.len() > MOST_ADDITIONS {
        problems.push(format!(
            "That's {} additions, and the most is {MOST_ADDITIONS}: keep only what the person showed matters most.",
            additions.len()
        ));
    }
    for (n, a) in additions.iter().enumerate() {
        let what = format!("Addition {}", n + 1);
        let (name, heading, body) = (text(a, "skill"), text(a, "heading"), text(a, "body"));
        let Some(skill) = skill_of(&name, &mut problems) else {
            continue;
        };
        let Some(sec) = section(a["section"].as_str().unwrap_or_default()) else {
            problems.push(format!(
                "{what}'s section must be howYouLikeIt, steps or watchOutFor."
            ));
            continue;
        };
        problems.extend(part_problem(&what, &heading, &body));
        if skill.part(&heading).is_some() {
            problems.push(format!(
                "{what}: {name} already has a part “{heading}”. Say so under same, or under disagrees if it says otherwise."
            ));
        }
        let after = Some(text(a, "after")).filter(|s| !s.is_empty());
        if let Some(after) = &after
            && skill.part(after).is_none()
        {
            problems.push(format!(
                "{what} goes after “{after}”, and {name} has no part with that heading. Give a heading exactly as written, or leave after empty."
            ));
        }
        learned.additions.push(Addition {
            skill: name,
            section: sec,
            after,
            heading,
            body,
            evidence: text(a, "evidence"),
        });
    }
    for (n, s) in list("same", &mut problems).iter().enumerate() {
        let (name, part) = (text(s, "skill"), text(s, "part"));
        let Some(skill) = skill_of(&name, &mut problems) else {
            continue;
        };
        if skill.part(&part).is_none() {
            problems.push(format!(
                "Same {}: {name} has no part “{part}”. Give its heading exactly as written.",
                n + 1
            ));
        }
        learned.same.push(Same {
            skill: name,
            part,
            evidence: text(s, "evidence"),
        });
    }
    for (n, d) in list("disagrees", &mut problems).iter().enumerate() {
        let what = format!("Disagrees {}", n + 1);
        let (name, part) = (text(d, "skill"), text(d, "part"));
        let (heading, body) = (text(d, "heading"), text(d, "body"));
        let Some(skill) = skill_of(&name, &mut problems) else {
            continue;
        };
        if skill.part(&part).is_none() {
            problems.push(format!(
                "{what}: {name} has no part “{part}”. Give its heading exactly as written."
            ));
        }
        problems.extend(part_problem(&what, &heading, &body));
        learned.disagrees.push(Disagrees {
            skill: name,
            part,
            heading,
            body,
            evidence: text(d, "evidence"),
        });
    }
    match &value["newSkill"] {
        Value::Null => {}
        Value::Object(_) => {
            let s = &value["newSkill"];
            let name = text(s, "name");
            let description = text(s, "description");
            let markdown = s["markdown"].as_str().unwrap_or_default().to_string();
            let kind = match s["kind"].as_str() {
                Some("do") => Some(skills::SkillKind::Do),
                Some("project") => Some(skills::SkillKind::Project),
                _ => {
                    problems.push("newSkill's kind must be do or project.".into());
                    None
                }
            };
            if names.contains(&name.as_str()) {
                problems.push(format!(
                    "newSkill “{name}” is a skill they have: add to it under additions instead."
                ));
            }
            problems.extend(skills::name_problem(&name));
            problems.extend(skills::description_problem(&description));
            for p in skills::check(&markdown) {
                problems.push(format!("newSkill: {p}"));
            }
            let draft = Skill::parse(&markdown);
            if draft.name().as_deref().map(str::trim) != Some(name.as_str()) {
                problems.push(format!(
                    "newSkill's markdown must say `name: {name}` in its front matter."
                ));
            }
            if draft.parts().is_empty() {
                problems.push("newSkill has no parts: each is a `###` heading under `## How you like it`, `## Steps` or `## Watch out for`.".into());
            }
            if let Some(kind) = kind {
                learned.new_skill = Some(NewSkill {
                    name,
                    description,
                    kind,
                    markdown,
                });
            }
        }
        _ => problems.push("newSkill must be an object, or left out.".into()),
    }
    if problems.is_empty() {
        Ok(learned)
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const RUST: &str = "---
name: rust
description: How Sam wants Rust written. Use when writing Rust.
---

# Rust

## How you like it

### Errors are one enum, with a message for each
Every way a function can fail is a variant.
";

    #[test]
    fn corrections_are_told_from_requests() {
        assert!(corrects("no, use a loop here"));
        assert!(corrects("That should exit 1, not 2"));
        assert!(corrects("don't add #[allow], fix the warning"));
        assert!(corrects("We use thiserror for errors"));
        assert!(!corrects("Add retries to the fetch command"));
        assert!(!corrects("Thanks, looks good"));
        assert!(!corrects(&"no ".repeat(400)));
    }

    #[test]
    fn something_new_goes_where_the_three_questions_say() {
        use WorkKind::*;
        assert_eq!(target(true, None, &[Rust]), Target::Project);
        assert_eq!(
            target(false, Some(("retrying-network-calls", 2)), &[Rust]),
            Target::Do("retrying-network-calls".into())
        );
        // Until a task has come up twice, its lines go where a default would.
        assert_eq!(
            target(false, Some(("retrying-network-calls", 1)), &[Rust]),
            Target::Like(Rust)
        );
        assert_eq!(target(false, None, &[Rust, Rust]), Target::Like(Rust));
        assert_eq!(
            target(false, None, &[Rust, TypeScript]),
            Target::Like(Everywhere)
        );
    }

    #[test]
    fn a_drafted_skill_and_a_projects_name_are_sound() {
        for kind in WorkKind::ALL {
            let draft = like_draft(kind);
            assert_eq!(skills::check(&draft), Vec::<String>::new(), "{draft}");
        }
        assert_eq!(
            project_name("/Users/sam/projects/Ledger_API/"),
            "ledger-api"
        );
        assert_eq!(project_name("/"), "project");
    }

    #[test]
    fn an_answer_names_only_skills_and_parts_it_was_given() {
        let given = [Given {
            name: "rust",
            text: RUST,
        }];
        let good = json!({
            "additions": [{
                "skill": "rust", "section": "howYouLikeIt",
                "after": "Errors are one enum, with a message for each",
                "heading": "Bad arguments exit 1, the tool's own failures exit 2",
                "body": "So scripts can tell a wrong call from a broken tool.",
                "evidence": "They said: that should exit 1",
            }],
            "same": [{ "skill": "rust", "part": "Errors are one enum, with a message for each", "evidence": "Kept the enum." }],
            "disagrees": [],
        });
        let learned = read(&good, &given).unwrap();
        assert_eq!(learned.additions.len(), 1);
        assert_eq!(learned.same.len(), 1);
        let bad = json!({
            "additions": [{
                "skill": "python", "section": "howYouLikeIt", "heading": "x", "body": "y", "evidence": "z",
            }, {
                "skill": "rust", "section": "rules", "heading": "x", "body": "y", "evidence": "z",
            }],
            "same": [{ "skill": "rust", "part": "No such part", "evidence": "" }],
            "disagrees": [],
        });
        let problems = read(&bad, &given).unwrap_err();
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems[0].contains("Use one of: rust"), "{problems:?}");
        assert!(
            read(
                &json!({"additions": [], "same": [], "disagrees": []}),
                &given
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn a_new_skill_must_be_sound_and_new() {
        let given = [Given {
            name: "rust",
            text: RUST,
        }];
        let answer = |markdown: &str| {
            json!({
                "additions": [], "same": [], "disagrees": [],
                "newSkill": {
                    "name": "retrying-network-calls",
                    "description": "Retrying network calls. Use when a network call can fail.",
                    "kind": "do",
                    "markdown": markdown,
                },
            })
        };
        let sound = "---\nname: retrying-network-calls\ndescription: Retrying network calls. Use when a network call can fail.\n---\n\n# Retrying network calls\n\n## Steps\n\n### Retry only what might pass next time\nTimeouts and 5xx, never 4xx.\n";
        assert!(read(&answer(sound), &given).unwrap().new_skill.is_some());
        let empty = "---\nname: retrying-network-calls\ndescription: Use when a network call can fail.\n---\n\n# Retrying\n";
        assert!(read(&answer(empty), &given).is_err());
    }
}
