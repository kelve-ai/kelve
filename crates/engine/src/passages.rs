//! A file cut into passages: what kelve marks, a passage at a time, and
//! keeps marks for by each passage's hash, so editing one function re-reads
//! that function and nothing else. Code is cut by its definitions, by the
//! grammar of its language; markdown by its sections; anything else by its
//! paragraphs. Every passage is whole lines, and passages never overlap.

use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

/// A passage smaller than this many lines joins its neighbours.
const SMALLEST: u32 = 8;
/// Passages grow towards this many lines when joined.
const TARGET: u32 = 60;
/// A passage longer than this is cut into smaller ones.
const LARGEST: u32 = 150;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Code,
    Prose,
}

/// Lines `start` to `end` of a file, counting from 1, both included.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passage {
    pub start: u32,
    pub end: u32,
    pub kind: Kind,
    /// What it defines or is headed, when it says: `reserve`, `Spending`.
    pub name: Option<String>,
    /// A short hash of its lines: the same lines, the same hash.
    pub hash: String,
}

impl Passage {
    /// Its lines, from the file's text.
    pub fn text<'a>(&self, text: &'a str) -> &'a str {
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let from = (self.start as usize).saturating_sub(1).min(lines.len());
        let to = (self.end as usize).min(lines.len());
        let start = lines[..from].iter().map(|l| l.len()).sum::<usize>();
        let len = lines[from..to].iter().map(|l| l.len()).sum::<usize>();
        &text[start..start + len]
    }
}

/// A span of lines before it's a passage.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Unit {
    start: u32,
    end: u32,
    name: Option<String>,
}

impl Unit {
    fn lines(&self) -> u32 {
        self.end + 1 - self.start
    }
}

/// The grammar for a file, by its name.
pub(crate) fn grammar(rel: &str) -> Option<tree_sitter::Language> {
    let ext = rel.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => tree_sitter_rust::LANGUAGE.into(),
        "ts" | "mts" | "cts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "js" | "jsx" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE.into(),
        "py" | "pyi" => tree_sitter_python::LANGUAGE.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        _ => return None,
    })
}

/// Whether a node is a comment, an attribute or a decorator: something that
/// belongs with what follows it.
fn leads(node: &Node) -> bool {
    matches!(
        node.kind(),
        "line_comment"
            | "block_comment"
            | "comment"
            | "attribute_item"
            | "inner_attribute_item"
            | "decorator"
    )
}

/// The name a definition gives itself, when it does.
fn name_of(node: &Node, src: &[u8]) -> Option<String> {
    let named = |n: &Node, field: &str| {
        n.child_by_field_name(field)
            .and_then(|c| c.utf8_text(src).ok())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty() && t.len() <= 80)
    };
    named(node, "name")
        .or_else(|| {
            node.child_by_field_name("declaration")
                .and_then(|d| name_of(&d, src))
        })
        .or_else(|| {
            node.child_by_field_name("definition")
                .and_then(|d| name_of(&d, src))
        })
        .or_else(|| named(node, "type"))
        .or_else(|| {
            let mut cursor = node.walk();
            let first = node
                .named_children(&mut cursor)
                .find(|c| c.kind() == "variable_declarator" || c.kind() == "type_spec");
            first.and_then(|c| named(&c, "name"))
        })
}

/// The body whose children a long definition is cut at: an `impl`'s or a
/// class's members.
fn body_of<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    if let Some(body) = node.child_by_field_name("body") {
        return Some(body);
    }
    for field in ["declaration", "definition"] {
        if let Some(inner) = node.child_by_field_name(field) {
            return body_of(&inner);
        }
    }
    None
}

fn line(row: usize) -> u32 {
    u32::try_from(row + 1).unwrap_or(u32::MAX)
}

/// The definitions among `nodes`, each with the comments and attributes
/// before it.
fn units_of(nodes: &[Node], src: &[u8]) -> Vec<Unit> {
    let mut units = Vec::new();
    let mut lead: Option<u32> = None;
    for node in nodes {
        let start = line(node.start_position().row);
        let end = line(node.end_position().row);
        if leads(node) {
            lead.get_or_insert(start);
            continue;
        }
        units.push(Unit {
            start: lead.take().unwrap_or(start),
            end,
            name: name_of(node, src),
        });
    }
    // Comments after the last definition stay with it.
    if let (Some(first), Some(last)) = (lead, nodes.last()) {
        let end = line(last.end_position().row);
        match units.last_mut() {
            Some(u) => u.end = u.end.max(end),
            None => units.push(Unit {
                start: first,
                end,
                name: None,
            }),
        }
    }
    units
}

/// Cuts a long definition at its members, the lines before the first
/// member staying with it; or, with no members to cut at, into even runs.
fn split(unit: Unit, node: Option<&Node>, src: &[u8], out: &mut Vec<Unit>) {
    if unit.lines() <= LARGEST {
        out.push(unit);
        return;
    }
    if let Some(body) = node.and_then(|n| body_of(n)) {
        let mut cursor = body.walk();
        let members: Vec<Node> = body.named_children(&mut cursor).collect();
        let mut parts = units_of(&members, src);
        if parts.len() > 1 {
            if let Some(first) = parts.first_mut() {
                first.start = unit.start;
            }
            if let Some(last) = parts.last_mut() {
                last.end = unit.end;
            }
            for (i, part) in parts.iter_mut().enumerate() {
                if part.name.is_none() && i == 0 {
                    part.name.clone_from(&unit.name);
                }
            }
            for part in parts {
                let member = members.iter().find(|m| {
                    line(m.start_position().row) >= part.start
                        && line(m.end_position().row) <= part.end
                        && !leads(m)
                });
                split(part, member, src, out);
            }
            return;
        }
    }
    runs(unit.start, unit.end, unit.name, out);
}

/// Even runs of lines, for anything too long with nothing to cut it at.
fn runs(start: u32, end: u32, name: Option<String>, out: &mut Vec<Unit>) {
    let mut at = start;
    while at <= end {
        let to = (at + TARGET - 1).min(end);
        out.push(Unit {
            start: at,
            end: to,
            name: name.clone(),
        });
        at = to + 1;
    }
}

/// Joins small neighbours into passages of about `TARGET` lines. A join
/// keeps the first name it has.
fn join(units: Vec<Unit>) -> Vec<Unit> {
    let mut out: Vec<Unit> = Vec::new();
    for unit in units {
        if let Some(last) = out.last_mut()
            && (last.lines() < SMALLEST || unit.lines() < SMALLEST)
            && unit.end + 1 - last.start <= TARGET
        {
            last.end = unit.end;
            if last.name.is_none() {
                last.name = unit.name;
            }
            continue;
        }
        out.push(unit);
    }
    out
}

/// Code cut by its grammar, or None when kelve has no grammar for it or it
/// doesn't parse.
fn code(rel: &str, text: &str) -> Option<Vec<Unit>> {
    let language = grammar(rel)?;
    let mut parser = Parser::new();
    parser.set_language(&language).ok()?;
    let tree = parser.parse(text, None)?;
    let root = tree.root_node();
    let src = text.as_bytes();
    let mut cursor = root.walk();
    let top: Vec<Node> = root.named_children(&mut cursor).collect();
    let mut units = Vec::new();
    for unit in units_of(&top, src) {
        let node = top
            .iter()
            .find(|n| !leads(n) && line(n.end_position().row) == unit.end);
        split(unit, node, src, &mut units);
    }
    Some(join(units))
}

/// Markdown cut at its headings, each section named by its heading.
fn markdown(text: &str) -> Vec<Unit> {
    use pulldown_cmark::{Event, Parser as Markdown, Tag, TagEnd};
    let line_at = |offset: usize| line(text[..offset.min(text.len())].matches('\n').count());
    let mut starts: Vec<(u32, Option<String>)> = vec![(1, None)];
    let mut heading: Option<(u32, String)> = None;
    for (event, range) in Markdown::new(text).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                heading = Some((line_at(range.start), String::new()))
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, words)) = heading.as_mut() {
                    words.push_str(&t);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((at, words)) = heading.take() {
                    let words = words.trim().to_string();
                    if at == 1 {
                        starts[0].1 = Some(words);
                    } else {
                        starts.push((at, Some(words).filter(|w| !w.is_empty())));
                    }
                }
            }
            _ => {}
        }
    }
    let last = line(text.lines().count().saturating_sub(1)).max(1);
    let mut units = Vec::new();
    for (i, (start, name)) in starts.iter().enumerate() {
        let end = starts.get(i + 1).map_or(last, |(next, _)| next - 1);
        if end >= *start {
            paragraphs(text, *start, end, name.clone(), &mut units);
        }
    }
    join(units)
}

/// Lines `start` to `end` cut into runs of about `TARGET` lines, at blank
/// lines where it can.
fn paragraphs(text: &str, start: u32, end: u32, name: Option<String>, out: &mut Vec<Unit>) {
    let lines: Vec<&str> = text.lines().collect();
    let blank = |n: u32| {
        lines
            .get(n as usize - 1)
            .is_none_or(|l| l.trim().is_empty())
    };
    let mut at = start;
    while at <= end {
        let mut to = (at + TARGET - 1).min(end);
        if to < end {
            // Back to a blank line, if there's one in the second half.
            if let Some(b) = (at + TARGET / 2..=to).rev().find(|n| blank(*n)) {
                to = b;
            }
        }
        out.push(Unit {
            start: at,
            end: to,
            name: if at == start { name.clone() } else { None },
        });
        at = to + 1;
    }
}

/// Whether a file is tests, by its name and where it is.
pub fn test_file(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let folder = lower
        .split('/')
        .rev()
        .skip(1)
        .any(|p| matches!(p, "tests" | "test" | "__tests__" | "e2e" | "spec"));
    folder
        || name.ends_with("_test.go")
        || name.ends_with("_test.py")
        || (name.starts_with("test_") && name.ends_with(".py"))
        || [
            ".test.ts",
            ".test.tsx",
            ".test.js",
            ".test.jsx",
            ".spec.ts",
            ".spec.tsx",
            ".spec.js",
        ]
        .iter()
        .any(|e| name.ends_with(e))
}

/// The lines of a file's tests: all of it in a file of tests, or each
/// Rust module under `#[cfg(test)]`, by the grammar. Tests aren't marked:
/// the ideas they rest on are the code's.
pub fn test_lines(rel: &str, text: &str) -> Vec<(u32, u32)> {
    if test_file(rel) {
        return vec![(1, u32::MAX)];
    }
    if !rel.ends_with(".rs") {
        return Vec::new();
    }
    let mut parser = Parser::new();
    let Some(tree) = grammar(rel)
        .filter(|g| parser.set_language(g).is_ok())
        .and_then(|_| parser.parse(text, None))
    else {
        return Vec::new();
    };
    let root = tree.root_node();
    let mut cursor = root.walk();
    let top: Vec<Node> = root.named_children(&mut cursor).collect();
    let mut out = Vec::new();
    for (i, node) in top.iter().enumerate() {
        if !matches!(node.kind(), "mod_item" | "function_item") {
            continue;
        }
        // The attributes just before it.
        let marked = top[..i]
            .iter()
            .rev()
            .take_while(|n| leads(n))
            .filter_map(|n| n.utf8_text(text.as_bytes()).ok())
            .any(|a| a.contains("cfg(test)") || a.contains("test]"));
        if marked {
            out.push((
                line(node.start_position().row),
                line(node.end_position().row),
            ));
        }
    }
    out
}

/// Whether a passage lies within a file's tests.
pub fn in_tests(tests: &[(u32, u32)], p: &Passage) -> bool {
    tests
        .iter()
        .any(|(from, to)| p.start >= *from && p.end <= *to)
        || tests.iter().any(|(from, to)| {
            // Mostly tests: a passage that starts just before a test
            // module, with its attribute, still counts.
            let overlap = p.end.min(*to).saturating_sub(p.start.max(*from)) + 1;
            p.end >= *from && p.start <= *to && overlap * 2 > p.end + 1 - p.start
        })
}

/// A file cut into passages, by its name and its text.
pub fn passages(rel: &str, text: &str) -> Vec<Passage> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let lang = kelve_core::code::language(rel);
    let prose = matches!(lang, Some("markdown"))
        || lang.is_none() && rel.to_ascii_lowercase().ends_with(".txt");
    let (units, kind) = if matches!(lang, Some("markdown")) {
        (markdown(text), Kind::Prose)
    } else if let Some(units) = code(rel, text) {
        (units, Kind::Code)
    } else {
        let last = line(text.lines().count().saturating_sub(1)).max(1);
        let mut units = Vec::new();
        paragraphs(text, 1, last, None, &mut units);
        (join(units), if prose { Kind::Prose } else { Kind::Code })
    };
    units
        .into_iter()
        .filter(|u| u.end >= u.start)
        .map(|u| {
            let mut p = Passage {
                start: u.start,
                end: u.end,
                kind,
                name: u.name,
                hash: String::new(),
            };
            p.hash = kelve_core::text::hash(p.text(text));
            p
        })
        .filter(|p| !p.text(text).trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_are_known_by_their_place_or_their_module() {
        assert_eq!(
            test_lines("crates/server/tests/local.rs", "fn a() {}"),
            [(1, u32::MAX)]
        );
        assert_eq!(
            test_lines("web/src/Entry.test.tsx", "it('works')"),
            [(1, u32::MAX)]
        );
        assert_eq!(
            test_lines("pkg/lock_test.go", "func TestLock() {}"),
            [(1, u32::MAX)]
        );
        let rust = "pub fn reserve() {}\n\n#[cfg(test)]\nmod tests {\n    fn helper() {}\n\n    #[test]\n    fn works() {}\n}\n";
        assert_eq!(test_lines("src/ledger.rs", rust), [(4, 9)]);
        assert!(test_lines("src/testing_utils.rs", "pub fn reserve() {}").is_empty());
    }

    fn spans(ps: &[Passage]) -> Vec<(u32, u32, Option<&str>)> {
        ps.iter()
            .map(|p| (p.start, p.end, p.name.as_deref()))
            .collect()
    }

    fn sound(ps: &[Passage]) {
        for w in ps.windows(2) {
            assert!(w[0].end < w[1].start, "{w:?} overlap");
        }
        for p in ps {
            assert!(p.start <= p.end, "{p:?}");
        }
    }

    #[test]
    fn rust_is_cut_by_definition_with_its_comments() {
        let mut text = String::from(
            "use std::fmt;\n\n/// Holds back the most a call could cost.\n#[derive(Debug)]\npub struct Hold {\n    id: i64,\n}\n\n",
        );
        for f in 0..3 {
            text.push_str(&format!("/// Function {f}.\npub fn f{f}() -> u32 {{\n"));
            for i in 0..10 {
                text.push_str(&format!("    let x{i} = {i};\n"));
            }
            text.push_str("    0\n}\n\n");
        }
        let ps = passages("src/ledger.rs", &text);
        sound(&ps);
        assert!(ps.iter().all(|p| p.kind == Kind::Code));
        // The small `use` and the struct join; each function stays whole,
        // with its doc comment.
        assert_eq!(ps[0].start, 1);
        let f1 = ps
            .iter()
            .find(|p| p.name.as_deref() == Some("f1"))
            .expect("f1");
        assert!(
            f1.text(&text).starts_with("/// Function 1."),
            "{}",
            f1.text(&text)
        );
        assert!(f1.text(&text).trim_end().ends_with('}'));
    }

    #[test]
    fn a_long_impl_is_cut_at_its_methods() {
        let mut text = String::from("impl Ledger {\n");
        for m in 0..6 {
            text.push_str(&format!("    /// Method {m}.\n    fn m{m}(&self) {{\n"));
            for i in 0..30 {
                text.push_str(&format!("        let x{i} = {i};\n"));
            }
            text.push_str("    }\n");
        }
        text.push_str("}\n");
        let ps = passages("src/ledger.rs", &text);
        sound(&ps);
        assert!(ps.len() >= 6, "{:?}", spans(&ps));
        assert!(ps.iter().all(|p| p.end + 1 - p.start <= LARGEST));
        assert_eq!(ps[0].start, 1, "the impl line stays with the first method");
        assert!(ps.iter().any(|p| p.name.as_deref() == Some("m3")));
    }

    #[test]
    fn typescript_python_and_go_are_cut_too() {
        let ts = "import x from 'y'\n\nexport function spend(n: number): number {\n  return n\n}\n\nexport class Ledger {\n  hold() {}\n}\n";
        assert!(!passages("web/src/a.tsx", ts).is_empty());
        let py =
            "def spend(n):\n    return n\n\nclass Ledger:\n    def hold(self):\n        pass\n";
        let ps = passages("ledger.py", py);
        sound(&ps);
        assert_eq!(ps[0].kind, Kind::Code);
        let go = "package main\n\nfunc Spend(n int) int {\n\treturn n\n}\n";
        assert!(!passages("main.go", go).is_empty());
    }

    #[test]
    fn markdown_is_cut_at_headings() {
        let mut text = String::from("# Spending\n\nkelve keeps a ledger.\n\n## Reserving\n\n");
        for i in 0..70 {
            text.push_str(&format!("Line {i} about reserving.\n"));
        }
        text.push_str("\n## Settling\n\nWhat it cost.\n");
        let ps = passages("docs/spending.md", &text);
        sound(&ps);
        assert!(ps.iter().all(|p| p.kind == Kind::Prose));
        assert!(
            ps.iter().any(|p| p.name.as_deref() == Some("Reserving")),
            "{:?}",
            spans(&ps)
        );
    }

    #[test]
    fn anything_else_is_cut_at_blank_lines_and_nothing_breaks_on_odd_input() {
        let text: String = (0..200)
            .map(|i| {
                if i % 20 == 0 {
                    "\n".to_string()
                } else {
                    format!("line {i}\n")
                }
            })
            .collect();
        let ps = passages("notes.txt", &text);
        sound(&ps);
        assert!(ps.iter().all(|p| p.kind == Kind::Prose));
        assert!(passages("a.rs", "").is_empty());
        assert!(
            passages("a.rs", "fn broken( {\n")
                .iter()
                .all(|p| p.start >= 1)
        );
        let p = &passages("a.rs", "fn a() {}")[0];
        assert_eq!(p.text("fn a() {}"), "fn a() {}");
    }

    #[test]
    fn the_same_lines_have_the_same_hash() {
        let a = passages("a.rs", "fn a() {}\n\n\n\n\n\n\n\n\n\nfn b() {}\n");
        let b = passages("a.rs", "fn a() {}\n\n\n\n\n\n\n\n\n\nfn b() {}\n");
        assert_eq!(a, b);
    }
}
