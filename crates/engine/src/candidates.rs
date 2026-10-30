//! What in a passage of code a mark can point at: its names, paths, calls
//! and strings, found by the grammar with their exact places, each with a
//! short id the model chooses by (`core::marks`). Comments aren't
//! candidates: they're prose, and a mark there quotes them. Imports and
//! attributes aren't either: a library's name is rarely the idea.

use kelve_core::marks::Candidate;
use tree_sitter::{Node, Parser, Tree};

use crate::passages::grammar;

/// The most candidates one passage offers: past this, the list costs more
/// than it helps.
pub const MOST: usize = 150;

/// What a mark can point at.
const NAMES: &[&str] = &[
    "identifier",
    "type_identifier",
    "field_identifier",
    "property_identifier",
    "shorthand_property_identifier",
    "package_identifier",
    "scoped_identifier",
    "scoped_type_identifier",
    "member_expression",
    "selector_expression",
    "attribute",
];

const STRINGS: &[&str] = &[
    "string_literal",
    "raw_string_literal",
    "string",
    "template_string",
    "interpreted_string_literal",
];

/// Imports, in each grammar.
const IMPORTS: &[&str] = &[
    "use_declaration",
    "extern_crate_declaration",
    "import_statement",
    "import_from_statement",
    "import_declaration",
];

/// What's never looked into, beside imports.
const SKIPPED: &[&str] = &[
    "line_comment",
    "block_comment",
    "comment",
    "attribute_item",
    "inner_attribute_item",
    "decorator",
];

/// Names too ordinary to point at.
const ORDINARY: &[&str] = &[
    "self",
    "Self",
    "this",
    "new",
    "len",
    "push",
    "get",
    "set",
    "map",
    "unwrap",
    "expect",
    "clone",
    "into",
    "from",
    "iter",
    "Some",
    "None",
    "Ok",
    "Err",
    "Result",
    "Option",
    "String",
    "Vec",
    "value",
    "data",
    "item",
    "items",
    "result",
    "res",
    "req",
    "error",
    "args",
    "ctx",
    "to_string",
    "as_str",
    "default",
    "main",
    "test",
    "await",
    "async",
    "true",
    "false",
];

/// The candidates in lines `start` to `end` of a file, in the order they
/// come, each text once. Empty for a language kelve has no grammar for.
pub fn candidates(rel: &str, text: &str, start: u32, end: u32) -> Vec<Candidate> {
    Parsed::new(rel, text).within(start, end)
}

/// A file parsed once, for the candidates in each of its passages: parsing
/// it again for every passage of a long file takes minutes.
#[derive(Debug)]
pub struct Parsed<'a> {
    text: &'a str,
    lines: Vec<&'a str>,
    tree: Option<Tree>,
}

impl<'a> Parsed<'a> {
    /// Parses a file, by its path for its language.
    pub fn new(rel: &str, text: &'a str) -> Parsed<'a> {
        let tree = grammar(rel).and_then(|language| {
            let mut parser = Parser::new();
            parser.set_language(&language).ok()?;
            parser.parse(text, None)
        });
        Parsed {
            text,
            lines: text.lines().collect(),
            tree,
        }
    }

    /// The candidates in lines `start` to `end`, in the order they come,
    /// each text once.
    pub fn within(&self, start: u32, end: u32) -> Vec<Candidate> {
        let Some(tree) = &self.tree else {
            return Vec::new();
        };
        let mut found: Vec<Candidate> = Vec::new();
        walk(
            tree.root_node(),
            self.text.as_bytes(),
            &self.lines,
            start,
            end,
            &mut found,
        );
        found
    }
}

fn line_of(row: usize) -> u32 {
    u32::try_from(row + 1).unwrap_or(u32::MAX)
}

fn walk(node: Node, src: &[u8], lines: &[&str], start: u32, end: u32, out: &mut Vec<Candidate>) {
    if out.len() >= MOST || SKIPPED.contains(&node.kind()) || IMPORTS.contains(&node.kind()) {
        return;
    }
    let (first, last) = (
        line_of(node.start_position().row),
        line_of(node.end_position().row),
    );
    if last < start || first > end {
        return;
    }
    let kind = node.kind();
    if first == last && first >= start && (NAMES.contains(&kind) || STRINGS.contains(&kind)) {
        offer(node, src, lines, STRINGS.contains(&kind), out);
    }
    // A string's parts aren't candidates of their own.
    if STRINGS.contains(&kind) {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, src, lines, start, end, out);
    }
}

fn offer(node: Node, src: &[u8], lines: &[&str], string: bool, out: &mut Vec<Candidate>) {
    let Ok(text) = node.utf8_text(src) else {
        return;
    };
    let chars = text.chars().count();
    let fits = if string {
        (4..=120).contains(&chars)
    } else {
        (3..=60).contains(&chars) && !ORDINARY.contains(&text)
    };
    if !fits || out.iter().any(|c| c.text == text) {
        return;
    }
    let row = node.start_position().row;
    let Some(line) = lines.get(row) else {
        return;
    };
    let col = |byte: usize| {
        line.get(..byte.min(line.len()))
            .map_or(0, |s| u32::try_from(s.chars().count()).unwrap_or(u32::MAX))
    };
    let (from, to) = (
        col(node.start_position().column),
        col(node.end_position().column),
    );
    if to <= from {
        return;
    }
    out.push(Candidate {
        id: format!("c{}", out.len() + 1),
        line: line_of(row),
        from,
        to,
        text: text.to_string(),
    });
}

/// A file's imports, a line each: what tells the model which library a
/// name comes from.
pub fn imports(rel: &str, text: &str) -> Vec<String> {
    let Some(language) = grammar(rel) else {
        return Vec::new();
    };
    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(text, None) else {
        return Vec::new();
    };
    let root = tree.root_node();
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .filter(|n| IMPORTS.contains(&n.kind()))
        .filter_map(|n| n.utf8_text(text.as_bytes()).ok())
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| t.chars().count() <= 160)
        .take(30)
        .collect()
}

/// What's part of a language itself, never a library it imports.
const OWN: &[&str] = &[
    "std",
    "core",
    "alloc",
    "crate",
    "self",
    "super",
    "proc_macro",
    "test",
    "__future__",
];

/// The libraries a file imports, by the name each is published under:
/// `use rand::Rng;` uses rand, `from requests.auth import …` requests,
/// `import x from '@scope/pkg/sub'` @scope/pkg. Never the language's own,
/// nor the work's own files (`./x`, `from . import`, `crate::`). Rust,
/// Python, JavaScript and TypeScript.
pub fn libraries(rel: &str, text: &str) -> Vec<String> {
    let ext = rel
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    for line in imports(rel, text) {
        let found: Vec<String> = match ext.as_str() {
            "rs" => {
                let l = line
                    .trim_start_matches("pub ")
                    .trim_start_matches("pub(crate) ");
                let rest = l
                    .strip_prefix("use ")
                    .or_else(|| l.strip_prefix("extern crate "))
                    .unwrap_or_default()
                    .trim_start_matches("::");
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                vec![name]
            }
            "py" | "pyi" => {
                if let Some(rest) = line.strip_prefix("from ") {
                    vec![
                        rest.split(['.', ' '])
                            .next()
                            .unwrap_or_default()
                            .to_string(),
                    ]
                } else if let Some(rest) = line.strip_prefix("import ") {
                    rest.split(',')
                        .map(|m| {
                            m.trim()
                                .split(['.', ' '])
                                .next()
                                .unwrap_or_default()
                                .to_string()
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            }
            "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => {
                let from = line
                    .rsplit_once(['\'', '"'])
                    .and_then(|(before, _)| before.rsplit_once(['\'', '"']))
                    .map(|(_, spec)| spec)
                    .unwrap_or_default();
                if from.starts_with(['.', '/']) || from.starts_with("node:") {
                    Vec::new()
                } else {
                    let mut parts = from.split('/');
                    let first = parts.next().unwrap_or_default();
                    vec![if first.starts_with('@') {
                        format!("{first}/{}", parts.next().unwrap_or_default())
                    } else {
                        first.to_string()
                    }]
                }
            }
            _ => Vec::new(),
        };
        for name in found {
            let name = name.trim().to_string();
            if !name.is_empty() && !OWN.contains(&name.as_str()) && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_libraries_a_file_imports() {
        let rust = "use rand::Rng;\nuse rand::seq::SliceRandom;\nuse std::io;\nuse crate::x;\npub use serde_json::{json, Value};\n";
        assert_eq!(libraries("src/lib.rs", rust), ["rand", "serde_json"]);
        let python = "import os.path, numpy as np\nfrom requests.auth import HTTPBasicAuth\nfrom . import local\n";
        assert_eq!(libraries("a.py", python), ["os", "numpy", "requests"]);
        let ts = "import { z } from 'zod'\nimport x from '@tanstack/react-query/build'\nimport './side'\nimport fs from 'node:fs'\n";
        assert_eq!(libraries("a.ts", ts), ["zod", "@tanstack/react-query"]);
        assert!(libraries("notes.md", "use rand::Rng;").is_empty());
    }

    const RUST: &str = "use sqlx::PgPool;\n\
        \n\
        /// Holds back the most a call could cost.\n\
        pub async fn reserve(tx: &mut Transaction) -> Result<i64> {\n\
            // Take the lock in the transaction.\n\
            sqlx::query(\"select pg_advisory_xact_lock($1)\").bind(KEY).execute(tx).await?;\n\
            Ok(1)\n\
        }\n";

    #[test]
    fn names_and_strings_with_their_exact_places() {
        let found = candidates("ledger.rs", RUST, 3, 8);
        let texts: Vec<&str> = found.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.contains(&"reserve"), "{texts:?}");
        assert!(texts.contains(&"Transaction"), "{texts:?}");
        assert!(texts.contains(&"sqlx::query"), "{texts:?}");
        assert!(
            texts.contains(&"\"select pg_advisory_xact_lock($1)\""),
            "{texts:?}"
        );
        assert!(!texts.contains(&"PgPool"), "imports aren't candidates");
        assert!(
            !texts.iter().any(|t| t.contains("Take the lock")),
            "comments are prose"
        );
        assert!(!texts.contains(&"Ok"), "too ordinary");
        for c in &found {
            let line = RUST.lines().nth(c.line as usize - 1).unwrap_or_default();
            let at: String = line
                .chars()
                .skip(c.from as usize)
                .take((c.to - c.from) as usize)
                .collect();
            assert_eq!(at, c.text, "{c:?}");
        }
        assert_eq!(imports("ledger.rs", RUST), ["use sqlx::PgPool;"]);
        assert!(candidates("notes.md", "# Locks\n", 1, 1).is_empty());
    }

    #[test]
    fn only_the_passages_lines() {
        let found = candidates("ledger.rs", RUST, 6, 6);
        assert!(found.iter().all(|c| c.line == 6), "{found:?}");
        assert!(!found.is_empty());
    }
}
