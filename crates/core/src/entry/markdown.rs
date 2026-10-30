//! An entry as markdown, for export: one file per entry, named by its date and
//! title, with a little front matter, every block written out, and its sources
//! as footnotes. A linked term is written as its words.
//!
//! kelve writes all of it from the blocks. A fold, closed in the reader
//! until it's opened, is written in full under a heading of its own, so an
//! entry's worked examples and common mistakes go wherever it does. A
//! diagram becomes a Mermaid
//! block kelve writes from the diagram's data, with its description under
//! it. Text is escaped so nothing in it can become a link, an image, HTML or
//! a different block in another tool, and `^1` becomes the footnote `[^1]`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::check::{Span, plain_https, spans};
use super::{Block, Body, Entry, Layout, Line, Node, Source};
use crate::formula;

/// The longest slug in a file name.
const SLUG: usize = 60;

/// An entry as a markdown file.
pub fn write(entry: &Entry) -> String {
    let mut out = vec![
        "---".to_string(),
        format!("title: {}", quoted(&entry.title)),
        format!("date: {}", entry.made_at.0.format("%Y-%m-%d")),
        format!("idea: {}", quoted(&entry.idea.name)),
        format!("id: {}", quoted(&entry.idea.id)),
        "---".to_string(),
        String::new(),
        format!("# {}", plain(&entry.title)),
        String::new(),
    ];
    let mut fold = None;
    for block in &entry.blocks {
        // A fold's blocks in a row are one fold, under one heading.
        let now = block.fold.map(|f| (block.part, f));
        if let Some((_, f)) = now
            && now != fold
        {
            out.push(format!("### {}", f.title()));
            out.push(String::new());
        }
        fold = now;
        out.extend(write_block(block));
    }
    for source in &entry.sources {
        out.push(footnote(source));
    }
    let mut text = out.join("\n");
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    format!("{}\n", text.trim_end())
}

/// An entry's file name: its date, then its title in plain ASCII letters,
/// digits and dashes, with apostrophes dropped:
/// `2026-10-12-why-docker-made-a-folder-you-couldnt-write-to.md`.
pub fn file_name(entry: &Entry) -> String {
    let mut slug = String::new();
    for c in entry.title.to_lowercase().chars() {
        if matches!(c, '\'' | '’' | '‘' | 'ʼ' | '`') {
            continue;
        }
        match fold(c) {
            Some(ascii) => slug.push_str(ascii),
            None if c.is_ascii_alphanumeric() => slug.push(c),
            None => slug.push('-'),
        }
    }
    let mut slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    slug.truncate(SLUG);
    let slug = slug.trim_end_matches('-');
    let slug = if slug.is_empty() {
        entry.idea.id.replace(':', "-")
    } else {
        slug.to_string()
    };
    format!("{}-{slug}.md", entry.made_at.0.format("%Y-%m-%d"))
}

/// Latin letters with accents, as the ASCII they're usually written as.
fn fold(c: char) -> Option<&'static str> {
    Some(match c {
        'à'..='å' | 'ā' | 'ă' | 'ą' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'č' => "c",
        'ď' | 'đ' | 'ð' => "d",
        'è'..='ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
        'ğ' => "g",
        'ì'..='ï' | 'ī' | 'į' | 'ı' => "i",
        'ł' | 'ľ' => "l",
        'ñ' | 'ń' | 'ň' => "n",
        'ò'..='ö' | 'ø' | 'ō' | 'ő' => "o",
        'œ' => "oe",
        'ř' => "r",
        'ś' | 'š' | 'ş' => "s",
        'ß' => "ss",
        'ť' | 'ţ' => "t",
        'þ' => "th",
        'ù'..='ü' | 'ū' | 'ů' | 'ű' | 'ų' => "u",
        'ý' | 'ÿ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => return None,
    })
}

fn write_block(block: &Block) -> Vec<String> {
    let mut out = Vec::new();
    match &block.body {
        Body::Paragraph { text, .. } => out.push(rich(text)),
        Body::Heading { text } => out.push(format!("## {}", plain(text))),
        Body::List { items } => out.extend(items.iter().map(|i| format!("- {}", rich(i)))),
        Body::Excerpt { from, lang, lines } => excerpt(&mut out, from, lang.as_deref(), lines),
        Body::Diagram {
            layout,
            nodes,
            edges,
            steps,
            describe,
        } => {
            out.push("```mermaid".to_string());
            out.extend(mermaid(*layout, nodes, edges));
            out.push("```".to_string());
            out.push(String::new());
            out.push(plain(describe));
            if !steps.is_empty() {
                out.push(String::new());
                for (i, step) in steps.iter().enumerate() {
                    out.push(format!("{}. {}", i + 1, plain(&step.caption)));
                }
            }
        }
        Body::Explorable {
            formula,
            inputs,
            output,
            describe,
        } => {
            out.push(format!("`{}`", formula::written(*formula)));
            out.push(String::new());
            let mut values = BTreeMap::new();
            for input in inputs {
                values.insert(input.name.clone(), input.value);
                let unit = input.unit.as_deref().map(unit_after).unwrap_or_default();
                out.push(format!(
                    "- {}: {}{unit} (from {} to {})",
                    plain(&input.label),
                    number(input.value),
                    number(input.min),
                    number(input.max),
                ));
            }
            if let Some(result) = formula::compute(*formula, &values) {
                let unit = formula::unit(*formula).map(unit_after).unwrap_or_default();
                out.push(String::new());
                out.push(format!("{}: {}{unit}", plain(output), number(result)));
            }
            out.push(String::new());
            out.push(plain(describe));
        }
        Body::Bars { rows, caption } => {
            for bar in rows {
                let label = plain(&bar.label);
                let label = if bar.mark {
                    format!("**{label}**")
                } else {
                    label
                };
                out.push(format!("- {label}: {}", plain(&bar.display)));
            }
            if let Some(caption) = caption {
                out.push(String::new());
                out.push(plain(caption));
            }
        }
        Body::Table { head, rows, marks } => {
            let width = head.len().max(1);
            let cells = |row: &[String], r: Option<usize>| {
                let cells: Vec<String> = (0..width)
                    .map(|c| {
                        let text = row.get(c).map(|t| cell(t)).unwrap_or_default();
                        let marked = r.is_some_and(|r| {
                            marks
                                .iter()
                                .any(|m| usize::from(m[0]) == r && usize::from(m[1]) == c)
                        });
                        if marked && !text.is_empty() {
                            format!("**{text}**")
                        } else {
                            text
                        }
                    })
                    .collect();
                format!("| {} |", cells.join(" | "))
            };
            out.push(cells(head, None));
            out.push(format!("|{}|", vec!["---"; width].join("|")));
            for (r, row) in rows.iter().enumerate() {
                out.push(cells(row, Some(r)));
            }
        }
        Body::Compare { left, right } => {
            for side in [left, right] {
                out.push(format!("**{}**", plain(&side.title)));
                out.push(String::new());
                out.extend(side.points.iter().map(|p| format!("- {}", rich(p))));
                out.push(String::new());
            }
        }
        Body::Squares { rows } => out.extend(rows.iter().map(|row| {
            let square = if row.red { '■' } else { '□' };
            format!("- {square} {}", rich(&row.text))
        })),
        Body::Timeline { rows } => out.extend(
            rows.iter()
                .map(|m| format!("- **{}:** {}", plain(&m.when), plain(&m.text))),
        ),
        Body::Conversation { lines } => {
            for said in lines {
                out.push(format!("**{}:** {}", plain(&said.who), rich(&said.text)));
                out.push(String::new());
            }
        }
        Body::Answer { lead, text } => out.push(format!("{} {}", plain(lead), rich(text))),
        Body::Sequence {
            messages, describe, ..
        } => {
            for (i, m) in messages.iter().enumerate() {
                let text = format!("{} → {}: {}", plain(&m.from), plain(&m.to), plain(&m.text));
                let text = if m.mark { format!("**{text}**") } else { text };
                out.push(format!("{}. {text}", i + 1));
            }
            out.push(String::new());
            out.push(plain(describe));
        }
        Body::Steps { cards } => {
            for (i, card) in cards.iter().enumerate() {
                out.push(format!(
                    "{}. **{}** {}",
                    i + 1,
                    plain(&card.title),
                    rich(&card.text)
                ));
            }
        }
        Body::Quadrant {
            x,
            y,
            points,
            describe,
        } => {
            for point in points {
                let across = if point.x >= 50.0 { &x.high } else { &x.low };
                let up = if point.y >= 50.0 { &y.high } else { &y.low };
                out.push(format!(
                    "- {}: {}, {}",
                    marked(&point.label, point.mark),
                    plain(across),
                    plain(up)
                ));
            }
            out.push(String::new());
            out.push(plain(describe));
        }
        Body::Spectrum {
            low,
            high,
            points,
            describe,
        } => {
            out.push(format!("From {} to {}:", plain(low), plain(high)));
            out.push(String::new());
            let mut points: Vec<_> = points.iter().collect();
            points.sort_by(|a, b| a.at.total_cmp(&b.at));
            for point in points {
                let note = point
                    .note
                    .as_deref()
                    .map(|n| format!(" ({})", plain(n)))
                    .unwrap_or_default();
                out.push(format!("- {}{note}", marked(&point.label, point.mark)));
            }
            out.push(String::new());
            out.push(plain(describe));
        }
        Body::Trend {
            labels,
            series,
            unit,
            describe,
        } => {
            let unit = unit.as_deref().map(unit_after).unwrap_or_default();
            let mut head = vec![String::new()];
            head.extend(labels.iter().map(|l| cell(l)));
            out.push(format!("| {} |", head.join(" | ")));
            out.push(format!("|{}|", vec!["---"; head.len()].join("|")));
            for line in series {
                let mut row = vec![marked(&line.label, line.mark)];
                row.extend(line.values.iter().map(|v| format!("{}{unit}", number(*v))));
                out.push(format!("| {} |", row.join(" | ")));
            }
            out.push(String::new());
            out.push(plain(describe));
        }
        Body::Tree { branches, describe } => {
            for branch in branches {
                let mut depth = 0;
                let mut parent = branch.parent.as_deref();
                while let Some(id) = parent
                    && depth < branches.len()
                {
                    depth += 1;
                    parent = branches
                        .iter()
                        .find(|b| b.id == id)
                        .and_then(|b| b.parent.as_deref());
                }
                let edge = branch
                    .edge
                    .as_deref()
                    .map(|e| format!("{}: ", plain(e)))
                    .unwrap_or_default();
                out.push(format!(
                    "{}- {edge}{}",
                    "  ".repeat(depth),
                    marked(&branch.label, branch.mark)
                ));
            }
            out.push(String::new());
            out.push(plain(describe));
        }
        Body::BeforeAfter {
            before,
            after,
            describe,
        } => {
            for version in [before, after] {
                out.push(format!("**{}**", plain(&version.label)));
                out.push(String::new());
                let lines: Vec<Line> = version
                    .lines
                    .iter()
                    .map(|t| Line {
                        n: None,
                        text: t.clone(),
                        mark: false,
                        dim: false,
                    })
                    .collect();
                excerpt(
                    &mut out,
                    version.from.as_deref().unwrap_or_default(),
                    version.lang.as_deref(),
                    &lines,
                );
                out.push(String::new());
            }
            out.push(plain(describe));
        }
        Body::Number { figure, text, .. } => {
            out.push(format!("**{}** {}", plain(figure), rich(text)));
        }
        Body::Annotated {
            from,
            lang,
            lines,
            notes,
        } => {
            let unmarked: Vec<Line> = lines
                .iter()
                .map(|l| Line {
                    mark: false,
                    ..l.clone()
                })
                .collect();
            excerpt(&mut out, from, lang.as_deref(), &unmarked);
            out.push(String::new());
            for (i, note) in notes.iter().enumerate() {
                let at = lines
                    .get(usize::from(note.line))
                    .and_then(|l| l.n)
                    .map(|n| format!("Line {n}: "))
                    .unwrap_or_default();
                out.push(format!("{}. {at}{}", i + 1, rich(&note.text)));
            }
        }
        Body::Funnel { stages, caption } => {
            for stage in stages {
                out.push(format!(
                    "- {}: {}",
                    marked(&stage.label, stage.mark),
                    plain(&stage.display)
                ));
            }
            if let Some(caption) = caption {
                out.push(String::new());
                out.push(plain(caption));
            }
        }
    }
    out.push(String::new());
    out
}

/// A label, in bold when it's the one that matters.
fn marked(label: &str, mark: bool) -> String {
    let label = plain(label);
    if mark { format!("**{label}**") } else { label }
}

/// Lines of the real thing, fenced so nothing in them is read as markdown,
/// with the line that matters named under them.
fn excerpt(out: &mut Vec<String>, from: &str, lang: Option<&str>, lines: &[Line]) {
    if !from.trim().is_empty() {
        out.push(code_span(from));
        out.push(String::new());
    }
    let longest = lines
        .iter()
        .map(|l| longest_run(&l.text, '`'))
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    // A code block says its language, so a notes app colours it too.
    let tag = lang
        .filter(|l| crate::code::LANGUAGES.contains(l))
        .unwrap_or_default();
    out.push(format!("{fence}{tag}"));
    out.extend(lines.iter().map(|l| one_line(&l.text)));
    out.push(fence);
    let marked: Vec<&Line> = lines.iter().filter(|l| l.mark).collect();
    let numbers: Option<Vec<String>> = marked.iter().map(|l| l.n.map(|n| n.to_string())).collect();
    match (marked.len(), numbers) {
        (0, _) => {}
        (1, Some(numbers)) => out.push(format!("\nThe line that matters is {}.", numbers[0])),
        (_, Some(numbers)) => out.push(format!("\nThe lines that matter are {}.", and(&numbers))),
        (_, None) => {
            let quoted: Vec<String> = marked
                .iter()
                .map(|l| code_span(&crate::text::clip(l.text.trim(), 60)))
                .collect();
            out.push(format!("\nThe line that matters: {}.", and(&quoted)));
        }
    }
}

/// A Mermaid flowchart of a diagram, written by kelve. Node ids are kelve's
/// own (`n1`, `n2`), and labels are quoted and escaped, so nothing in them
/// is read as Mermaid.
fn mermaid(layout: Layout, nodes: &[Node], edges: &[super::Edge]) -> Vec<String> {
    let direction = match layout {
        Layout::Stack => "TB",
        Layout::Row | Layout::Cycle | Layout::Timeline => "LR",
    };
    let mut out = vec![format!("flowchart {direction}")];
    let id = |name: &str| {
        nodes
            .iter()
            .position(|n| n.id == name)
            .map(|i| format!("n{}", i + 1))
    };
    for (i, node) in nodes.iter().enumerate() {
        let label = match &node.note {
            Some(note) if !note.trim().is_empty() => format!("{} ({})", node.label, note),
            _ => node.label.clone(),
        };
        out.push(format!("  n{}[\"{}\"]", i + 1, mermaid_text(&label)));
    }
    for edge in edges {
        let (Some(from), Some(to)) = (id(&edge.from), id(&edge.to)) else {
            continue;
        };
        match &edge.label {
            Some(label) if !label.trim().is_empty() => {
                out.push(format!("  {from} -->|\"{}\"| {to}", mermaid_text(label)));
            }
            _ => out.push(format!("  {from} --> {to}")),
        }
    }
    let marked: Vec<String> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.mark)
        .map(|(i, _)| format!("n{}", i + 1))
        .collect();
    if !marked.is_empty() {
        out.push("  classDef mark stroke:#c8102e,stroke-width:3px".to_string());
        out.push(format!("  class {} mark", marked.join(",")));
    }
    out
}

/// Text inside a quoted Mermaid label, with everything Mermaid or HTML
/// might read written as an entity code.
fn mermaid_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in one_line(text).chars() {
        match c {
            '#' => out.push_str("#35;"),
            '"' => out.push_str("#quot;"),
            '<' => out.push_str("#lt;"),
            '>' => out.push_str("#gt;"),
            '`' => out.push_str("#96;"),
            '&' => out.push_str("#amp;"),
            c => out.push(c),
        }
    }
    out
}

/// A source as a footnote: `[^1]: Volumes. Docker, 2024. <https://…>`.
fn footnote(source: &Source) -> String {
    let mut line = format!(
        "[^{}]: {}.",
        source.n,
        plain(source.title.trim().trim_end_matches('.'))
    );
    let by = [
        source.publisher.trim(),
        source.published.as_deref().unwrap_or("").trim(),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .map(plain)
    .collect::<Vec<_>>();
    if !by.is_empty() {
        let _ = write!(line, " {}.", by.join(", "));
    }
    if source.preprint {
        line.push_str(" A preprint.");
    }
    if let Some(url) = source.url.as_deref().filter(|u| plain_https(u)) {
        let _ = write!(line, " <{url}>");
    }
    if let Some(archived) = source.archived.as_deref().filter(|u| plain_https(u)) {
        let _ = write!(line, " Archived at <{archived}>.");
    }
    line
}

/// Prose as markdown: code spans kept, `^1` as the footnote `[^1]`, and
/// everything else that markdown would act on escaped.
fn rich(text: &str) -> String {
    let text = super::check::LINKED
        .replace_all(&one_line(text), "$1")
        .into_owned();
    let pieces = spans(&text).unwrap_or_else(|| vec![Span::Text(&text)]);
    let mut out = String::with_capacity(text.len() + 8);
    for piece in pieces {
        match piece {
            Span::Code(code) => out.push_str(code),
            Span::Text(prose) => escape(&mut out, prose, true),
        }
    }
    guard(out.trim())
}

/// Plain text as markdown, with everything markdown would act on escaped.
fn plain(text: &str) -> String {
    let mut out = String::new();
    escape(&mut out, &one_line(text), false);
    guard(out.trim())
}

/// A table cell: plain, and without a `|` that would end it.
fn cell(text: &str) -> String {
    plain(text).replace('|', "\\|")
}

fn escape(out: &mut String, text: &str, prose: bool) {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if prose && c == '*' && next == Some('*') {
            out.push_str("**");
            i += 2;
            continue;
        }
        if prose && c == '^' && next.is_some_and(|d| d.is_ascii_digit()) {
            let digits: String = chars[i + 1..]
                .iter()
                .take_while(|d| d.is_ascii_digit())
                .collect();
            let _ = write!(out, "[^{digits}]");
            i += 1 + digits.len();
            continue;
        }
        let tag = c == '<' && next.is_some_and(|n| n.is_ascii_alphabetic() || "/!?".contains(n));
        if matches!(c, '\\' | '*' | '[' | ']') || tag || (!prose && c == '`') {
            out.push('\\');
        }
        out.push(c);
        i += 1;
    }
}

/// Text that can't start a different block: a heading, a quote, a list, a
/// rule, a table or a fence.
fn guard(text: &str) -> String {
    let starts_block =
        text.starts_with(['#', '>', '-', '+', '=', '|', '~']) || text.starts_with("```");
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    let numbered = digits > 0 && text[digits..].starts_with(['.', ')']);
    if starts_block {
        format!("\\{text}")
    } else if numbered {
        format!("{}\\{}", &text[..digits], &text[digits..])
    } else {
        text.to_string()
    }
}

/// Text on one line: line breaks and other control characters as spaces.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() && c != '\t' { ' ' } else { c })
        .collect()
}

/// Text as a code span, fenced with more backticks than it holds.
fn code_span(text: &str) -> String {
    let text = one_line(text);
    let fence = "`".repeat(longest_run(&text, '`') + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

fn longest_run(text: &str, of: char) -> usize {
    let (mut longest, mut run) = (0, 0);
    for c in text.chars() {
        run = if c == of { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    longest
}

/// A string in front matter, quoted as JSON, which YAML reads too.
fn quoted(text: &str) -> String {
    serde_json::to_string(&one_line(text)).unwrap_or_else(|_| "\"\"".to_string())
}

/// "3", "3 and 7", "3, 7 and 9".
fn and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A unit after a number: `%` straight after it, words after a space.
fn unit_after(unit: &str) -> String {
    let unit = plain(unit);
    if unit.is_empty() || unit == "%" {
        unit
    } else {
        format!(" {unit}")
    }
}

/// A number as people write it: `1,169.18`, `0.52`, `200,000`.
fn number(x: f64) -> String {
    if !x.is_finite() {
        return "–".to_string();
    }
    if x.abs() >= 1e15 {
        return format!("{x:e}");
    }
    let rounded = (x * 100.0).round() / 100.0;
    let text = format!("{:.2}", rounded.abs());
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    let mut grouped = String::new();
    for (i, digit) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    let fraction = fraction.trim_end_matches('0');
    let sign = if rounded < 0.0 { "-" } else { "" };
    if fraction.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{fraction}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{
        Bar, CodeNote, Edge, Formula, Frame, FunnelStage, Input, Mark, Moment, QuadrantAxis,
        QuadrantPoint, Said, SequenceMessage, Side, SpectrumPoint, Square, StepCard, TreeBranch,
        TrendSeries, Version,
    };
    use crate::time::Timestamp;

    fn block(id: &str, body: Body) -> Block {
        Block {
            id: id.into(),
            part: None,
            fold: None,
            body,
        }
    }

    fn entry(title: &str, blocks: Vec<Block>, sources: Vec<Source>) -> Entry {
        Entry {
            idea: crate::ideas::IdeaRef {
                id: "kelve:i0000000000vx".into(),
                name: "Docker volumes".into(),
            },
            title: title.into(),
            question: String::new(),
            minutes: 1,
            made_at: Timestamp::parse("2026-10-12T09:30:00Z").unwrap_or_else(Timestamp::now),
            blocks,
            sources,
            links: Vec::new(),
        }
    }

    fn volumes() -> Source {
        Source {
            n: 1,
            title: "Volumes".into(),
            publisher: "Docker".into(),
            url: Some("https://docs.docker.com/engine/storage/volumes/".into()),
            published: Some("2024".into()),
            preprint: false,
            archived: None,
        }
    }

    fn docker_back() -> Entry {
        let excerpt = block(
            "b3",
            Body::Excerpt {
                lang: Some("dockerfile".into()),
                from: "Dockerfile".into(),
                lines: vec![
                    Line {
                        n: Some(35),
                        text: "    && useradd --system --home-dir /data kelve \\".into(),
                        mark: false,
                        dim: false,
                    },
                    Line {
                        n: Some(36),
                        text: "    && mkdir /data && chown kelve /data".into(),
                        mark: true,
                        dim: false,
                    },
                    Line {
                        n: None,
                        text: "…".into(),
                        mark: false,
                        dim: true,
                    },
                    Line {
                        n: Some(44),
                        text: "VOLUME /data".into(),
                        mark: false,
                        dim: false,
                    },
                ],
            },
        );
        entry(
            "Your own Dockerfile already knew this",
            vec![
                block(
                    "b1",
                    Body::Paragraph {
                        text: "kelve's compose file gives it a [[named volume]], `data:/data`, and it runs as **its own user**^1.".into(),
                        quiet: false,
                    },
                ),
                excerpt,
                block(
                    "b4",
                    Body::Answer {
                        lead: "So if anyone asks:".into(),
                        text: "Docker makes a missing folder as root.".into(),
                    },
                ),
            ],
            vec![volumes()],
        )
    }

    #[test]
    fn an_entry_reads_as_markdown() {
        let text = write(&docker_back());
        let expected = "---
title: \"Your own Dockerfile already knew this\"
date: 2026-10-12
idea: \"Docker volumes\"
id: \"kelve:i0000000000vx\"
---

# Your own Dockerfile already knew this

kelve's compose file gives it a named volume, `data:/data`, and it runs as **its own user**[^1].

`Dockerfile`

```dockerfile
    && useradd --system --home-dir /data kelve \\
    && mkdir /data && chown kelve /data
…
VOLUME /data
```

The line that matters is 36.

So if anyone asks: Docker makes a missing folder as root.

[^1]: Volumes. Docker, 2024. <https://docs.docker.com/engine/storage/volumes/>
";
        assert_eq!(text, expected);
    }

    #[test]
    fn file_names_are_plain() {
        let e = entry(
            "Why Docker made a folder you couldn’t write to",
            vec![],
            vec![],
        );
        assert_eq!(
            file_name(&e),
            "2026-10-12-why-docker-made-a-folder-you-couldnt-write-to.md"
        );
        let e = entry("Où est la café? Naïve “quotes” & ümlauts!", vec![], vec![]);
        assert_eq!(
            file_name(&e),
            "2026-10-12-ou-est-la-cafe-naive-quotes-umlauts.md"
        );
        let long = "word ".repeat(30);
        let name = file_name(&entry(&long, vec![], vec![]));
        let slug = name
            .trim_start_matches("2026-10-12-")
            .trim_end_matches(".md");
        assert!(slug.len() <= SLUG && !slug.ends_with('-'), "{name}");
        let e = entry("日本語のタイトル", vec![], vec![]);
        assert_eq!(file_name(&e), "2026-10-12-kelve-i0000000000vx.md");
        let e = entry("../../etc/passwd", vec![], vec![]);
        assert_eq!(file_name(&e), "2026-10-12-etc-passwd.md");
    }

    #[test]
    fn diagrams_become_mermaid_kelve_writes() {
        let node = |id: &str, label: &str, mark: bool| Node {
            id: id.into(),
            label: label.into(),
            note: None,
            mark,
        };
        let diagram = block(
            "b1",
            Body::Diagram {
                layout: Layout::Row,
                nodes: vec![
                    node("image", "The image", false),
                    node("volume", "A \"new\" volume <b>#1</b>", true),
                ],
                edges: vec![
                    Edge {
                        from: "image".into(),
                        to: "volume".into(),
                        label: Some("copies in".into()),
                    },
                    Edge {
                        from: "image".into(),
                        to: "ghost".into(),
                        label: None,
                    },
                ],
                steps: vec![Frame {
                    caption: "The volume mounts.".into(),
                    marks: vec!["volume".into()],
                }],
                describe: "The image's folder is copied into the volume.".into(),
            },
        );
        let text = write(&entry("A diagram", vec![diagram], vec![]));
        assert!(
            text.contains(
                "```mermaid
flowchart LR
  n1[\"The image\"]
  n2[\"A #quot;new#quot; volume #lt;b#gt;#35;1#lt;/b#gt;\"]
  n1 -->|\"copies in\"| n2
  classDef mark stroke:#c8102e,stroke-width:3px
  class n2 mark
```

The image's folder is copied into the volume.

1. The volume mounts."
            ),
            "{text}"
        );
        assert!(!text.contains("ghost"));
    }

    #[test]
    fn explorables_write_their_formula_and_numbers() {
        let input = |name: &str, label: &str, value: f64, max: f64, unit: Option<&str>| Input {
            name: name.into(),
            label: label.into(),
            min: 0.0,
            max,
            step: 1.0,
            value,
            unit: unit.map(str::to_string),
        };
        let explorable = block(
            "b1",
            Body::Explorable {
                formula: Formula::LoanPayment,
                inputs: vec![
                    input(
                        "principal",
                        "How much you borrow",
                        200_000.0,
                        500_000.0,
                        Some("£"),
                    ),
                    input("rate", "The rate", 5.0, 15.0, Some("%")),
                    input("years", "Over", 25.0, 35.0, Some("years")),
                ],
                output: "Monthly payment".into(),
                describe: "Move the rate to see the payment change.".into(),
            },
        );
        let text = write(&entry("Your mortgage", vec![explorable], vec![]));
        for line in [
            "`payment = principal × r ÷ (1 − (1 + r)^−m), where r is the yearly rate ÷ 12 and m is years × 12`",
            "- How much you borrow: 200,000 £ (from 0 to 500,000)",
            "- The rate: 5% (from 0 to 15)",
            "- Over: 25 years (from 0 to 35)",
            "Monthly payment: 1,169.18",
            "Move the rate to see the payment change.",
        ] {
            assert!(text.contains(line), "missing {line:?} in\n{text}");
        }
    }

    #[test]
    fn every_other_block_writes() {
        let blocks = vec![
            block(
                "b1",
                Body::Heading {
                    text: "They picked the bigger change".into(),
                },
            ),
            block(
                "b2",
                Body::List {
                    items: vec!["One^1".into(), "1. Not a numbered list".into()],
                },
            ),
            block(
                "b3",
                Body::Bars {
                    rows: vec![Bar {
                        label: "your context".into(),
                        value: 46.0,
                        display: "2,180".into(),
                        mark: true,
                        quiet: false,
                    }],
                    caption: Some("Lines added.".into()),
                },
            ),
            block(
                "b4",
                Body::Table {
                    head: vec!["Judges lean towards".into(), "In your test".into()],
                    rows: vec![vec!["Longer answers".into(), "Not checked | yet".into()]],
                    marks: vec![[0, 1]],
                },
            ),
            block(
                "b5",
                Body::Compare {
                    left: Side {
                        title: "In the image".into(),
                        points: vec!["Costs nothing".into()],
                    },
                    right: Side {
                        title: "At start".into(),
                        points: vec!["Runs every time".into()],
                    },
                },
            ),
            block(
                "b6",
                Body::Squares {
                    rows: vec![
                        Square {
                            red: false,
                            text: "the image has a folder".into(),
                        },
                        Square {
                            red: true,
                            text: "**Docker makes it, as root**".into(),
                        },
                    ],
                },
            ),
            block(
                "b7",
                Body::Timeline {
                    rows: vec![Moment {
                        when: "Friday".into(),
                        text: "The cache broke.".into(),
                        mark: Mark::Red,
                    }],
                },
            ),
            block(
                "b8",
                Body::Conversation {
                    lines: vec![Said {
                        who: "Sam".into(),
                        text: "Why not fix it at start?".into(),
                    }],
                },
            ),
        ];
        let text = write(&entry("Everything", blocks, vec![volumes()]));
        for line in [
            "## They picked the bigger change",
            "- One[^1]",
            "- 1\\. Not a numbered list",
            "- **your context**: 2,180",
            "Lines added.",
            "| Judges lean towards | In your test |",
            "|---|---|",
            "| Longer answers | **Not checked \\| yet** |",
            "**In the image**",
            "- Costs nothing",
            "- □ the image has a folder",
            "- ■ **Docker makes it, as root**",
            "- **Friday:** The cache broke.",
            "**Sam:** Why not fix it at start?",
        ] {
            assert!(text.contains(line), "missing {line:?} in\n{text}");
        }
    }

    #[test]
    fn the_newer_blocks_write() {
        let line = |n: u32, text: &str, mark: bool| Line {
            n: Some(n),
            text: text.into(),
            mark,
            dim: false,
        };
        let e = entry(
            "Volumes, a block at a time",
            vec![
                block(
                    "b1",
                    Body::Sequence {
                        actors: vec!["You".into(), "Docker".into()],
                        messages: vec![SequenceMessage {
                            from: "You".into(),
                            to: "Docker".into(),
                            text: "run -v".into(),
                            mark: true,
                        }],
                        describe: "You ask Docker.".into(),
                    },
                ),
                block(
                    "b2",
                    Body::Steps {
                        cards: vec![StepCard {
                            title: "Read".into(),
                            text: "It reads `compose.yml`.".into(),
                        }],
                    },
                ),
                block(
                    "b3",
                    Body::Quadrant {
                        x: QuadrantAxis {
                            low: "harder".into(),
                            high: "easier".into(),
                        },
                        y: QuadrantAxis {
                            low: "less control".into(),
                            high: "more control".into(),
                        },
                        points: vec![QuadrantPoint {
                            label: "Compose".into(),
                            x: 80.0,
                            y: 70.0,
                            mark: false,
                        }],
                        describe: "Compose is easy.".into(),
                    },
                ),
                block(
                    "b4",
                    Body::Spectrum {
                        low: "a feeling".into(),
                        high: "a suite".into(),
                        points: vec![
                            SpectrumPoint {
                                label: "Ten cases".into(),
                                at: 45.0,
                                note: Some("you".into()),
                                mark: true,
                            },
                            SpectrumPoint {
                                label: "Eyeballing".into(),
                                at: 5.0,
                                note: None,
                                mark: false,
                            },
                        ],
                        describe: "In the middle.".into(),
                    },
                ),
                block(
                    "b5",
                    Body::Trend {
                        labels: vec!["2020".into(), "2024".into()],
                        series: vec![TrendSeries {
                            label: "Use Compose".into(),
                            values: vec![40.0, 68.5],
                            mark: false,
                        }],
                        unit: Some("%".into()),
                        describe: "Up.".into(),
                    },
                ),
                block(
                    "b6",
                    Body::Tree {
                        branches: vec![
                            TreeBranch {
                                id: "t1".into(),
                                label: "Docker?".into(),
                                parent: None,
                                edge: None,
                                mark: false,
                            },
                            TreeBranch {
                                id: "t2".into(),
                                label: "compose up".into(),
                                parent: Some("t1".into()),
                                edge: Some("yes".into()),
                                mark: false,
                            },
                        ],
                        describe: "Ask first.".into(),
                    },
                ),
                block(
                    "b7",
                    Body::BeforeAfter {
                        before: Version {
                            label: "Before".into(),
                            from: Some("Dockerfile".into()),
                            lang: Some("dockerfile".into()),
                            lines: vec!["VOLUME /data".into()],
                        },
                        after: Version {
                            label: "After".into(),
                            from: None,
                            lang: Some("yaml".into()),
                            lines: vec!["volumes: [data:/data]".into()],
                        },
                        describe: "A named volume.".into(),
                    },
                ),
                block(
                    "b8",
                    Body::Number {
                        figure: "1 in 3".into(),
                        text: "setups stop at the database^1.".into(),
                        share: Some(33.0),
                    },
                ),
                block(
                    "b9",
                    Body::Annotated {
                        from: "Dockerfile".into(),
                        lang: Some("dockerfile".into()),
                        lines: vec![
                            line(39, "USER kelve", false),
                            line(44, "VOLUME /data", true),
                        ],
                        notes: vec![CodeNote {
                            line: 1,
                            text: "The volume is made here.".into(),
                        }],
                    },
                ),
                block(
                    "b11",
                    Body::Funnel {
                        stages: vec![
                            FunnelStage {
                                label: "Visit".into(),
                                value: 100.0,
                                display: "100".into(),
                                mark: false,
                            },
                            FunnelStage {
                                label: "Run it".into(),
                                value: 41.0,
                                display: "41".into(),
                                mark: true,
                            },
                        ],
                        caption: Some("Out of 100.".into()),
                    },
                ),
            ],
            vec![volumes()],
        );
        let md = write(&e);
        for expected in [
            "1. **You → Docker: run -v**",
            "1. **Read** It reads `compose.yml`.",
            "- Compose: easier, more control",
            "From a feeling to a suite:\n\n- Eyeballing\n- **Ten cases** (you)",
            "|  | 2020 | 2024 |",
            "| Use Compose | 40% | 68.5% |",
            "- Docker?\n  - yes: compose up",
            "**Before**\n\n`Dockerfile`\n\n```dockerfile\nVOLUME /data\n```",
            "```yaml\nvolumes: [data:/data]\n```",
            "**1 in 3** setups stop at the database[^1].",
            "```dockerfile\nUSER kelve\nVOLUME /data\n```\n\n1. Line 44: The volume is made here.",
            "- Visit: 100\n- **Run it**: 41\n\nOut of 100.",
        ] {
            assert!(md.contains(expected), "no {expected:?} in:\n{md}");
        }
    }

    #[test]
    fn nothing_becomes_a_link_html_or_a_new_block() {
        let mut bad = volumes();
        bad.title = "[Click](javascript:alert(1)) <img src=x onerror=alert(1)>\n# Owned".into();
        bad.publisher = "*Evil*".into();
        bad.url = Some("javascript:alert(1)".into());
        bad.archived = Some("https://web.archive.org/web/2024/https://docs.docker.com/".into());
        let blocks = vec![
            block(
                "b1",
                Body::Paragraph {
                    text: "# Not a heading, [not](a link) and <b>not bold</b>".into(),
                    quiet: false,
                },
            ),
            block(
                "b2",
                Body::Paragraph {
                    text: "```\nnot a fence".into(),
                    quiet: false,
                },
            ),
            block(
                "b3",
                Body::Excerpt {
                    lang: None,
                    from: "a `weird` file".into(),
                    lines: vec![Line {
                        n: None,
                        text: "```".into(),
                        mark: true,
                        dim: false,
                    }],
                },
            ),
        ];
        let text = write(&entry("<script>alert(1)</script>", blocks, vec![bad]));
        assert!(
            text.contains("title: \"<script>alert(1)</script>\""),
            "{text}"
        );
        assert!(text.contains("# \\<script>alert(1)\\</script>"), "{text}");
        assert!(
            text.contains("\\# Not a heading, \\[not\\](a link) and \\<b>not bold\\</b>"),
            "{text}"
        );
        assert!(
            text.contains("\\```   not a fence") || text.contains("\\``` not a fence"),
            "{text}"
        );
        assert!(text.contains("``a `weird` file``"), "{text}");
        assert!(text.contains("````\n```\n````"), "{text}");
        assert!(text.contains(
            "[^1]: \\[Click\\](javascript:alert(1)) \\<img src=x onerror=alert(1)> # Owned. \\*Evil\\*, 2024. Archived at <https://web.archive.org/web/2024/https://docs.docker.com/>."
        ), "{text}");
        assert!(!text.contains("<javascript"));
    }

    #[test]
    fn numbers_read_as_people_write_them() {
        assert_eq!(number(1169.1846), "1,169.18");
        assert_eq!(number(200_000.0), "200,000");
        assert_eq!(number(0.52), "0.52");
        assert_eq!(number(-1234.5), "-1,234.5");
        assert_eq!(number(-0.001), "0");
        assert_eq!(number(f64::NAN), "–");
        assert_eq!(number(12.0), "12");
    }

    #[test]
    fn a_fold_is_written_in_full_under_a_heading_of_its_own() {
        use crate::entry::{Fold, Part};
        let para = |id: &str, part: Part, fold: Option<Fold>, text: &str| Block {
            id: id.into(),
            part: Some(part),
            fold,
            body: Body::Paragraph {
                text: text.into(),
                quiet: false,
            },
        };
        let e = entry(
            "Exponential backoff",
            vec![
                para("b1", Part::Lead, None, "Each wait is longer than the last."),
                para(
                    "b2",
                    Part::How,
                    None,
                    "A client waits a short time, then twice that.",
                ),
                para(
                    "b3",
                    Part::How,
                    Some(Fold::Example),
                    "Say the base wait is 1 second.",
                ),
                para("b4", Part::How, Some(Fold::Example), "Then 2, 4, 8 and 16."),
                para(
                    "b5",
                    Part::How,
                    Some(Fold::Rule),
                    "Each wait is base times two to the try.",
                ),
                para("b6", Part::Limits, None, "On its own it isn't enough."),
            ],
            Vec::new(),
        );
        let md = write(&e);
        let example = md
            .find("### A worked example")
            .expect("the example's heading");
        assert_eq!(
            md.matches("### A worked example").count(),
            1,
            "one fold, one heading: {md}"
        );
        assert!(
            md[example..].starts_with(
                "### A worked example\n\nSay the base wait is 1 second.\n\nThen 2, 4, 8 and 16."
            ),
            "{md}"
        );
        let rule = md.find("### The precise rule").expect("the rule's heading");
        assert!(rule > example);
        // What's always shown has no heading of its own.
        assert!(md.find("On its own").unwrap() > rule);
        assert_eq!(md.matches("### ").count(), 2);
    }
}
