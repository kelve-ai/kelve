//! Reading the markdown agents and people write, as far as kelve needs to:
//! its lines, which of them are code, its headings, its front matter, and
//! whether it keeps to the few kinds of markdown kelve shows. Never a full
//! markdown parser: the web app renders, kelve only checks and splits.

use std::sync::LazyLock;

use regex::Regex;

use crate::text::re;

/// One line of a markdown text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line<'a> {
    /// Its number, from 1.
    pub number: usize,
    /// The line without its line break.
    pub text: &'a str,
    /// The line with its line break, if it has one: every `raw`, joined,
    /// is the text again.
    pub raw: &'a str,
    /// Whether it's code: a fence, or a line inside one.
    pub code: bool,
}

/// Every line of a text, each with whether it's code. An unclosed fence runs
/// to the end, as markdown has it.
pub fn lines(text: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for (i, raw) in text.split_inclusive('\n').enumerate() {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let opens = fence_of(line);
        let code = match (fence, opens) {
            (Some((c, n)), Some((d, m))) if c == d && m >= n && closes(line) => {
                fence = None;
                true
            }
            (Some(_), _) => true,
            (None, Some(f)) => {
                fence = Some(f);
                true
            }
            (None, None) => false,
        };
        out.push(Line {
            number: i + 1,
            text: line,
            raw,
            code,
        });
    }
    out
}

/// The fence a line opens or closes, if it's one: its character and how
/// many of it.
fn fence_of(line: &str) -> Option<(char, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let c = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = rest.chars().take_while(|x| *x == c).count();
    (n >= 3).then_some((c, n))
}

/// Whether a fence line can close a fence: nothing after its marks.
fn closes(line: &str) -> bool {
    let t = line.trim();
    t.chars().all(|c| c == '`') || t.chars().all(|c| c == '~')
}

/// A heading, if the line is one, outside code: its level, 1 to 6, and its
/// words, without the marks that open or close it.
pub fn heading(line: &str) -> Option<(usize, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let level = rest.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let after = &rest[level..];
    if !(after.is_empty() || after.starts_with(' ') || after.starts_with('\t')) {
        return None;
    }
    let words = after.trim();
    // A closing run of #s, after a space, isn't part of the heading.
    let words = match words.trim_end_matches('#') {
        w if w.len() < words.len() && (w.is_empty() || w.ends_with(' ')) => w.trim_end(),
        _ => words,
    };
    Some((level, words))
}

/// A text's front matter: the lines between a first `---` and the next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Front {
    /// Each field, in order, its value unquoted, and a value written over
    /// several lines joined.
    pub fields: Vec<(String, String)>,
    /// How many bytes of the text it takes, both `---` lines included.
    pub len: usize,
    /// How many lines it takes.
    pub lines: usize,
}

impl Front {
    /// A field's value, by its name.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The front matter a text starts with, if it does, and it's closed.
pub fn front_matter(text: &str) -> Option<Front> {
    let all = lines(text);
    let first = all.first()?;
    if first.text.trim_end() != "---" {
        return None;
    }
    let end = all
        .iter()
        .skip(1)
        .position(|l| matches!(l.text.trim_end(), "---" | "..."))?
        + 1;
    let len = all[..=end].iter().map(|l| l.raw.len()).sum();
    let mut fields: Vec<(String, String)> = Vec::new();
    // How a field written over several lines joins them: folded with
    // spaces, or kept with line breaks.
    let mut block: Option<char> = None;
    for line in &all[1..end] {
        let indented = line.text.starts_with(' ') || line.text.starts_with('\t');
        if indented || line.text.trim().is_empty() {
            if let Some((_, value)) = fields.last_mut() {
                let more = line.text.trim();
                if more.is_empty() {
                    if block == Some('|') {
                        value.push('\n');
                    }
                    continue;
                }
                if !value.is_empty() && !value.ends_with('\n') {
                    value.push(if block == Some('|') { '\n' } else { ' ' });
                }
                value.push_str(more);
            }
            continue;
        }
        let Some((key, value)) = line.text.split_once(':') else {
            continue;
        };
        let value = value.trim();
        block = match value {
            "|" | "|-" | "|+" => Some('|'),
            ">" | ">-" | ">+" => Some('>'),
            _ => None,
        };
        let value = if block.is_some() {
            String::new()
        } else {
            unquote(value)
        };
        fields.push((key.trim().to_string(), value));
    }
    for (_, value) in &mut fields {
        *value = value.trim().to_string();
    }
    Some(Front {
        fields,
        len,
        lines: end + 1,
    })
}

/// A value without the quotes around it, if it has them.
fn unquote(value: &str) -> String {
    let v = value.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        return v[1..v.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }
    if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        return v[1..v.len() - 1].replace("''", "'");
    }
    v.to_string()
}

/// A value as front matter writes it: plain when it can be, in double
/// quotes when it can't.
pub fn quote(value: &str) -> String {
    let value = value.replace(['\n', '\r'], " ");
    let plain = !value.is_empty()
        && !value.contains(": ")
        && !value.contains(" #")
        && !value.ends_with(':')
        && !value.starts_with(|c: char| "!&*[]{}|>'\"%@`#,?-:".contains(c))
        && value.trim() == value;
    if plain {
        value
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// A line with what's inside its code spans blanked, so nothing in code is
/// taken for markup.
pub fn without_code_spans(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '`' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let run = chars[i..].iter().take_while(|c| **c == '`').count();
        // Where a run of the same length closes it.
        let mut j = i + run;
        let mut close = None;
        while j < chars.len() {
            if chars[j] == '`' {
                let n = chars[j..].iter().take_while(|c| **c == '`').count();
                if n == run {
                    close = Some(j);
                    break;
                }
                j += n;
            } else {
                j += 1;
            }
        }
        match close {
            Some(j) => {
                out.extend(std::iter::repeat_n('`', run));
                out.extend(std::iter::repeat_n(' ', j - (i + run)));
                out.extend(std::iter::repeat_n('`', run));
                i = j + run;
            }
            None => {
                out.extend(std::iter::repeat_n('`', run));
                i += run;
            }
        }
    }
    out
}

static HTML: LazyLock<Regex> = LazyLock::new(|| {
    re(r"<!--|<\s*/?\s*[A-Za-z][A-Za-z0-9-]*(?:\s[^<>]*)?/?\s*>|&[A-Za-z]+;|&#\d+;")
});
static AUTOLINK: LazyLock<Regex> = LazyLock::new(|| re(r"<([A-Za-z][A-Za-z0-9+.-]*):[^\s<>]*>"));
static LINK: LazyLock<Regex> = LazyLock::new(|| re(r"\]\(\s*<?([^)\s>]*)"));
static REFERENCE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\s{0,3}\[[^\]]+\]:\s*<?(\S*?)>?(?:\s|$)"));
static QUOTE: LazyLock<Regex> = LazyLock::new(|| re(r"^\s{0,3}>"));
static RULE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\s{0,3}(?:(?:-\s*){3,}|(?:\*\s*){3,}|(?:_\s*){3,})$"));

/// Whether a link goes to a source on the web.
fn web(target: &str) -> bool {
    let t = target.to_ascii_lowercase();
    (t.starts_with("https://") || t.starts_with("http://")) && t.len() > "https://".len()
}

/// What's wrong with a text kelve will show, against the markdown it
/// allows: headings, paragraphs, lists, tables, code, bold, italics, code
/// spans and links to sources on the web. No HTML, no images, no other
/// links, no quotes or rules. Each problem is a sentence that says what to
/// change, by line number counted from `first`.
pub fn problems(text: &str, first: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines(text) {
        if line.code {
            continue;
        }
        let n = line.number + first - 1;
        let plain = without_code_spans(line.text);
        if let Some(m) = HTML.find(&plain) {
            out.push(format!(
                "Line {n} has HTML in it, {}. Write markdown, and put code in backticks.",
                quoted(m.as_str())
            ));
        }
        if plain.contains("![") {
            out.push(format!(
                "Line {n} has an image in it. Leave images out: kelve shows words and code only."
            ));
        }
        for c in LINK.captures_iter(&plain) {
            let target = c.get(1).map_or("", |m| m.as_str());
            if !web(target) {
                out.push(format!(
                    "Line {n} links to {}. Link only to sources on the web, with https://, or leave the link out.",
                    if target.is_empty() { "nothing".to_string() } else { quoted(target) }
                ));
            }
        }
        if let Some(c) = REFERENCE.captures(&plain) {
            let target = c.get(1).map_or("", |m| m.as_str());
            if !web(target) {
                out.push(format!(
                    "Line {n} links to {}. Link only to sources on the web, with https://, or leave the link out.",
                    quoted(target)
                ));
            }
        }
        for c in AUTOLINK.captures_iter(&plain) {
            let scheme = c.get(1).map_or("", |m| m.as_str()).to_ascii_lowercase();
            if scheme != "http" && scheme != "https" {
                out.push(format!(
                    "Line {n} links to {}. Link only to sources on the web, with https://, or leave the link out.",
                    quoted(c.get(0).map_or("", |m| m.as_str()))
                ));
            }
        }
        if QUOTE.is_match(&plain) {
            out.push(format!(
                "Line {n} is a quote block. Write it as a paragraph, or put what's quoted in a code block."
            ));
        }
        if RULE.is_match(&plain) {
            out.push(format!(
                "Line {n} is a line of dashes or stars. Leave it out: use a heading to start something new."
            ));
        }
    }
    out
}

/// A short piece of text, quoted in a problem.
fn quoted(found: &str) -> String {
    format!("“{}”", crate::text::clip(found.trim(), 40))
}

/// How many words a text has outside its code blocks.
pub fn words(text: &str) -> usize {
    lines(text)
        .iter()
        .filter(|l| !l.code)
        .map(|l| {
            l.text
                .split_whitespace()
                .filter(|w| w.chars().any(char::is_alphanumeric))
                .count()
        })
        .sum()
}

/// Absolute paths on someone's computer: `/Users/…`, `/home/…`, `C:\…`,
/// `~/…`, where a path begins.
pub(crate) static ABSOLUTE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?:^|[\s"'`(\[=,])(?:[A-Za-z]:\\|~/|/(?:Users|home|private|var|tmp|opt|root|mnt|Volumes)/)"#,
    )
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_known_by_its_fences() {
        let text = "a\n```rust\n# not a heading\n```\n## b\n~~~\n```\nstill code\n~~~\nafter";
        let got: Vec<(usize, bool)> = lines(text).iter().map(|l| (l.number, l.code)).collect();
        assert_eq!(
            got,
            vec![
                (1, false),
                (2, true),
                (3, true),
                (4, true),
                (5, false),
                (6, true),
                (7, true),
                (8, true),
                (9, true),
                (10, false)
            ]
        );
        // The raw lines are the text again.
        let joined: String = lines(text).iter().map(|l| l.raw).collect();
        assert_eq!(joined, text);
        // An unclosed fence runs to the end.
        assert!(lines("```\n# x\n").iter().all(|l| l.code));
    }

    #[test]
    fn headings_have_a_level_and_words() {
        assert_eq!(heading("# Rust"), Some((1, "Rust")));
        assert_eq!(
            heading("### Errors are one enum ###"),
            Some((3, "Errors are one enum"))
        );
        assert_eq!(heading("   ## Steps"), Some((2, "Steps")));
        assert_eq!(heading("###"), Some((3, "")));
        assert_eq!(heading("#hashtag"), None);
        assert_eq!(heading("    # indented code"), None);
        assert_eq!(heading("####### seven"), None);
        assert_eq!(heading("### C#"), Some((3, "C#")));
    }

    #[test]
    fn front_matter_is_read_with_its_values_unquoted() {
        let text = "---\nname: rust\ndescription: \"How Sam wants Rust: plainly.\"\nother: 'it''s'\n---\n# Rust\n";
        let front = front_matter(text).unwrap();
        assert_eq!(front.get("name"), Some("rust"));
        assert_eq!(
            front.get("description"),
            Some("How Sam wants Rust: plainly.")
        );
        assert_eq!(front.get("other"), Some("it's"));
        assert_eq!(&text[front.len..], "# Rust\n");
        assert_eq!(front.lines, 5);

        let folded = "---\nname: x\ndescription: >\n  Use when\n  writing Rust.\n---\n";
        assert_eq!(
            front_matter(folded).unwrap().get("description"),
            Some("Use when writing Rust.")
        );
        let plain = "---\ndescription: Use when\n  writing Rust.\n---\n";
        assert_eq!(
            front_matter(plain).unwrap().get("description"),
            Some("Use when writing Rust.")
        );
        assert_eq!(front_matter("# No front matter\n"), None);
        assert_eq!(front_matter("---\nname: never closed\n"), None);
    }

    #[test]
    fn values_are_quoted_only_when_they_must_be() {
        assert_eq!(quote("How Sam wants Rust."), "How Sam wants Rust.");
        assert_eq!(quote("Use when: writing"), "\"Use when: writing\"");
        assert_eq!(quote("say \"hi\""), "say \"hi\"");
        assert_eq!(quote("- list"), "\"- list\"");
        let front = front_matter(&format!("---\nd: {}\n---\n", quote("a: \"b\" \\ c"))).unwrap();
        assert_eq!(front.get("d"), Some("a: \"b\" \\ c"));
    }

    #[test]
    fn code_spans_hide_what_they_hold() {
        assert_eq!(without_code_spans("a `<div>` b"), "a `     ` b");
        assert_eq!(without_code_spans("``a ` b`` c"), "``     `` c");
        assert_eq!(without_code_spans("an ` unclosed"), "an ` unclosed");
    }

    #[test]
    fn only_the_markdown_kelve_shows_passes() {
        let fine = "#### Heading\n\nA **bold** and *italic* `<code>` line, with [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110).\n\n- a list\n1. numbered\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```html\n<div>fine in code</div>\n```\n\nSee <https://example.com>. 3 < 4 and 5 > 2.";
        assert_eq!(problems(fine, 1), Vec::<String>::new());

        let bad = [
            ("<div>hi</div>", "HTML"),
            ("Use Vec<String> here", "HTML"),
            ("<!-- note -->", "HTML"),
            ("&nbsp;", "HTML"),
            ("![logo](https://x.com/a.png)", "image"),
            ("[file](src/main.rs)", "links to"),
            ("[run](javascript:alert(1))", "links to"),
            ("[x]: file:///etc/passwd", "links to"),
            ("<mailto:sam@example.com>", "links to"),
            ("> quoted", "quote block"),
            ("---", "dashes"),
        ];
        for (text, says) in bad {
            let got = problems(text, 10);
            assert_eq!(got.len(), 1, "{text}: {got:?}");
            assert!(got[0].contains(says), "{text}: {got:?}");
            assert!(got[0].starts_with("Line 10 "), "{got:?}");
        }
    }

    #[test]
    fn words_leave_out_code() {
        assert_eq!(
            words("One two three.\n```\nlet a = b;\n```\nfour - five"),
            5
        );
    }

    #[test]
    fn absolute_paths_are_found_where_they_begin() {
        assert!(ABSOLUTE.is_match("see /Users/sam/acme/src"));
        assert!(ABSOLUTE.is_match("C:\\Users\\sam"));
        assert!(ABSOLUTE.is_match("in ~/projects"));
        assert!(!ABSOLUTE.is_match("src/retry.rs and https://example.com/home/x"));
    }
}
