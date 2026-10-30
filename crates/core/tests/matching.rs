//! How well kelve matches lessons to moments, across kinds of work
//! (`matching.json`): of the moments where a lesson fits, how often the one
//! given first is it; and of all moments, how often one is given when none
//! fits. A change to matching that makes either worse fails here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use kelve_core::lesson::{Lesson, Record, Standing, seen_as};
use kelve_core::matching::{At, Candidate, Card, Context, card, choose, fingerprint};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LessonData {
    says: String,
    #[serde(rename = "do")]
    act: String,
    holds_when: String,
    #[serde(default)]
    seen_as: Option<String>,
}

#[derive(Deserialize, Default)]
struct ContextData {
    #[serde(default)]
    versions: BTreeMap<String, String>,
    #[serde(default)]
    settings: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Case {
    kind: String,
    at: String,
    #[serde(default)]
    command: String,
    #[serde(default)]
    problem: String,
    #[serde(default)]
    written: String,
    #[serde(default)]
    file: String,
    #[serde(default)]
    context: ContextData,
    want: Option<String>,
}

#[derive(Deserialize)]
struct Bench {
    lessons: BTreeMap<String, LessonData>,
    cases: Vec<Case>,
}

#[test]
fn lessons_are_matched_precisely_and_nothing_is_given_when_none_fits() {
    let bench: Bench = serde_json::from_str(include_str!("matching.json")).unwrap();
    let lessons: Vec<(String, Vec<String>, Card)> = bench
        .lessons
        .iter()
        .map(|(id, l)| {
            let lesson = Lesson {
                about: vec![format!("kelve:i{id}")],
                says: l.says.clone(),
                act: l.act.clone(),
                example: None,
                holds_when: l.holds_when.clone(),
                backed_by: None,
                seen_as: l.seen_as.as_deref().map(seen_as),
            };
            (id.clone(), lesson.about.clone(), card(&lesson))
        })
        .collect();
    let has_seen: BTreeMap<&str, bool> = bench
        .lessons
        .iter()
        .map(|(id, l)| (id.as_str(), l.seen_as.is_some()))
        .collect();
    let candidates: Vec<Candidate> = lessons
        .iter()
        .map(|(id, about, card)| Candidate {
            id,
            about,
            card,
            record: Record::default(),
            standing: Standing::New,
            seen_as: has_seen[id.as_str()],
        })
        .collect();
    let mut n = 0.0_f64;
    let mut draw = || {
        n = (n + 0.618_033_988_7) % 1.0;
        n
    };
    let (mut fits, mut right, mut none, mut false_gives) = (0, 0, 0, 0);
    let mut wrong: Vec<String> = Vec::new();
    for case in &bench.cases {
        let ctx = Context {
            versions: case.context.versions.clone(),
            settings: case.context.settings.clone(),
        };
        let print = fingerprint(&case.command, &seen_as(&case.problem));
        let at = match case.at.as_str() {
            "failed" => At::Failed(&print),
            "edited" => At::Edited {
                written: &case.written,
                file: &case.file,
            },
            _ => At::Read { file: &case.file },
        };
        let got = choose(at, &candidates, &ctx, 1, &mut draw);
        let first = got.first().map(|c| c.id.clone());
        match &case.want {
            Some(want) => {
                fits += 1;
                if first.as_deref() == Some(want) {
                    right += 1;
                } else {
                    wrong.push(format!(
                        "{} {}: wanted {want}, gave {first:?}",
                        case.kind, case.at
                    ));
                }
            }
            None => {
                none += 1;
                if let Some(f) = first {
                    false_gives += 1;
                    wrong.push(format!(
                        "{} {}: wanted nothing, gave {f}",
                        case.kind, case.at
                    ));
                }
            }
        }
    }
    let precision = f64::from(right) / f64::from(fits.max(1));
    let false_rate = f64::from(false_gives) / f64::from(none.max(1));
    eprintln!(
        "precision@1 {precision:.2} ({right}/{fits}); false gives {false_rate:.2} ({false_gives}/{none})"
    );
    for w in &wrong {
        eprintln!("  {w}");
    }
    assert!(
        precision >= 0.75,
        "precision@1 fell to {precision:.2}: {wrong:#?}"
    );
    assert!(
        false_rate <= 0.1,
        "lessons given where none fits: {false_rate:.2}: {wrong:#?}"
    );
}
