//! Taking secrets and other people's details out of text, and deciding
//! whether a search query may leave the computer.
//!
//! `redact` runs at every door: in the command line before a hook sends
//! anything, on the server before anything is stored, and again before
//! anything goes to a model. Each secret becomes `[secret]`, and nothing of
//! it is kept, not even a hash. The rules lean towards catching a secret
//! over keeping a line intact, but ordinary code and prose come through
//! unchanged: `let token = parse(input);` and `max_tokens: 4096` are not
//! secrets.

use std::sync::LazyLock;

use regex::{Captures, Regex, RegexSet};
use serde_json::Value;
use thiserror::Error;

use crate::text::re;

/// What a secret becomes.
pub const SECRET: &str = "[secret]";

/// One kind of secret: a pattern, what a find becomes, and, for patterns
/// that would otherwise catch ordinary code, a test the `value` group must
/// pass, given the character after the find.
struct Rule {
    pattern: String,
    replace: &'static str,
    test: Option<fn(&str, Option<char>) -> bool>,
}

fn rule(pattern: impl Into<String>) -> Rule {
    Rule {
        pattern: pattern.into(),
        replace: SECRET,
        test: None,
    }
}

fn keep(pattern: impl Into<String>, replace: &'static str) -> Rule {
    Rule {
        pattern: pattern.into(),
        replace,
        test: None,
    }
}

fn tested(
    pattern: impl Into<String>,
    replace: &'static str,
    test: fn(&str, Option<char>) -> bool,
) -> Rule {
    Rule {
        pattern: pattern.into(),
        replace,
        test: Some(test),
    }
}

/// Words that name a secret, in assignments of every form.
const SECRET_NAME: &str = r"(?:password|passwd|passphrase|secret|token|api[_-]?key|apikey|access[_-]?key|private[_-]?key|client[_-]?secret|credentials?)";

/// The rules, most specific first: known token formats, then headers and
/// connection strings, then assignments to names that sound secret.
static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    vec![
        // Private keys, whole, or cut off by a head-and-tail excerpt.
        rule(
            r"-----BEGIN[ A-Z0-9]*PRIVATE KEY(?: BLOCK)?-----(?:[\s\S]*?-----END[ A-Z0-9]*PRIVATE KEY(?: BLOCK)?-----|(?:(?:[ \t]*(?:\r?\n|\\[rn]))+(?:[A-Za-z0-9+/]{16,}={0,2}|[A-Za-z0-9+/]{1,15}={1,2}|[A-Za-z-]+: [^\n\\]*))*)",
        ),
        // AWS access key ids.
        rule(r"\b(?:AKIA|ASIA|ABIA|ACCA|A3T[A-Z0-9])[A-Z0-9]{16}\b"),
        // GitHub.
        rule(r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,})"),
        // GitLab.
        rule(
            r"\b(?:gl(?:pat|dt|ptt|rt|cbt|soat|ft|imt|oas|wt)-[A-Za-z0-9_-]{20,}|glagent-[A-Za-z0-9_-]{20,}|GR1348941[A-Za-z0-9_-]{20,})",
        ),
        // Slack tokens, and Slack and Discord webhooks.
        rule(r"\b(?:xox[abposre]-[A-Za-z0-9-]{10,}|xapp-\d-[A-Za-z0-9-]{10,})"),
        rule(
            r"(?i)https://(?:hooks\.slack\.com/(?:services|workflows|triggers)|(?:ptb\.|canary\.)?discord(?:app)?\.com/api/webhooks)/[A-Za-z0-9/_-]+",
        ),
        // Stripe.
        rule(r"\b(?:(?:sk|rk|pk)_(?:live|test)_[A-Za-z0-9]{10,}|whsec_[A-Za-z0-9]{20,})"),
        // Anthropic, OpenRouter, then OpenAI and everyone else who starts
        // keys with `sk-`.
        rule(r"\bsk-ant-[A-Za-z0-9_-]{20,}"),
        rule(r"\bsk-or-[A-Za-z0-9_-]{20,}"),
        tested(
            r"\bsk-(?P<value>[A-Za-z0-9_-]{20,})",
            SECRET,
            random_looking,
        ),
        // Google: API keys, OAuth client secrets, access and refresh tokens.
        rule(
            r"\b(?:AIza[0-9A-Za-z_-]{30,}|GOCSPX-[A-Za-z0-9_-]{20,}|ya29\.[A-Za-z0-9_-]{20,}|1//0[A-Za-z0-9_-]{30,})",
        ),
        // Brave Search.
        tested(
            r"\b(?P<value>BSA[A-Za-z0-9_-]{20,})",
            SECRET,
            random_looking,
        ),
        // Hugging Face, Replicate, Groq, xAI, Perplexity.
        rule(r"\b(?:hf_|r8_|gsk_|xai-|pplx-)[A-Za-z0-9]{30,}"),
        // npm and PyPI.
        rule(r"\b(?:npm_[A-Za-z0-9]{36}\b|pypi-[A-Za-z0-9_-]{50,})"),
        // SendGrid, Mailgun, Twilio.
        rule(r"\bSG\.[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,}"),
        rule(r"\bkey-[0-9a-f]{32}\b"),
        rule(r"\bSK[0-9a-f]{32}\b"),
        // Discord bot tokens and Telegram bot tokens.
        rule(r"\b[MNO][A-Za-z0-9_-]{23,27}\.[A-Za-z0-9_-]{6}\.[A-Za-z0-9_-]{27,40}\b"),
        rule(r"\b\d{8,10}:AA[A-Za-z0-9_-]{33}\b"),
        // Shopify, DigitalOcean, Linear, Supabase, Notion, Postman,
        // Doppler, Vault.
        rule(
            r"\b(?:shp(?:at|ca|pa|ss)_[a-fA-F0-9]{32}|do[opr]_v1_[a-f0-9]{64}|lin_api_[A-Za-z0-9]{40}|sbp_[a-f0-9]{40}|(?:secret|ntn)_[A-Za-z0-9]{40,}|PMAK-[a-f0-9]{24}-[a-f0-9]{34}|dp\.(?:pt|st|sa|ct|scim|audit)\.[A-Za-z0-9]{40,}|hv[sb]\.[A-Za-z0-9_-]{24,})",
        ),
        // Azure connection strings.
        keep(
            r"(?i)\b((?:AccountKey|SharedAccessKey)=)[A-Za-z0-9+/=]{20,}",
            "${1}[secret]",
        ),
        // JSON web tokens: a header and a payload, both JSON.
        rule(r"\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]*"),
        // A password in an address: `postgres://kelve:hunter2@db`.
        keep(
            r"(?i)\b([a-z][a-z0-9+.-]{1,20}://)[^\s/?#@:'\x22<>]*:[^\s/?#@'\x22<>]+@",
            "${1}[secret]@",
        ),
        // Authorization headers, with or without their scheme.
        tested(
            r"(?i)\b(authorization[\x22']?\s*[:=]\s*[\x22'\x60]?)(?:(?:bearer|basic|token|digest|negotiate|bot)\s+)?(?P<value>[^\s\x22',;]{6,})",
            "${1}[secret]",
            random_looking,
        ),
        tested(
            r"(?i)\b(bearer\s+)(?P<value>[A-Za-z0-9._~+/-]{16,}=*)",
            "${1}[secret]",
            random_looking,
        ),
        // Other headers that carry a key.
        tested(
            r"(?i)\b((?:x-api-key|x-subscription-token|x-auth-token|x-access-token|x-goog-api-key|api-key|private-token)[\x22']?\s*[:=]\s*[\x22']?)(?P<value>[^\s\x22',;]{6,})",
            "${1}[secret]",
            not_placeholder,
        ),
        tested(
            r"(?i)\b((?:set-)?cookie[\x22']?\s*:\s*[\x22']?)(?P<value>[^\n\x22']{8,})",
            "${1}[secret]",
            cookie,
        ),
        // `curl -u name:password`.
        keep(
            r"(?m)((?:^|\s)(?:-u|--user)[\s=]+[\x22']?)[^\s:\x22']+:[^\s\x22']+",
            "${1}[secret]",
        ),
        // `.npmrc`.
        tested(
            r"(?m)(:_(?:authToken|auth|password)\s*=\s*)(?P<value>\S+)",
            "${1}[secret]",
            not_placeholder,
        ),
        // `.env` files and `export NAME=…`.
        tested(
            r"(?m)^(\s*(?:export\s+)?[A-Z0-9_]*(?:PASSWORD|PASSWD|PASSPHRASE|SECRET|TOKEN|API_?KEY|ACCESS_?KEY|PRIVATE_?KEY|CREDENTIALS?)[A-Z0-9_]*\s*=\s*[\x22']?)(?P<value>[^\s\x22']+)",
            "${1}[secret]",
            env_value,
        ),
        // Command-line flags: `--password hunter2`, `--token $TOKEN` (kept).
        tested(
            r"(?i)((?:^|\s)--?(?:password|passwd|pass|token|api-key|apikey|secret|client-secret|access-token|auth-token)\s+[\x22']?)(?P<value>[^\s\x22'-][^\s\x22']*)",
            "${1}[secret]",
            not_placeholder,
        ),
        // Quoted values given to secret names, in JSON, YAML, TOML and
        // code: `"api_key": "…"`, `password = '…'`.
        tested(
            format!(
                r#"(?i)(\b[A-Za-z0-9_.-]*?{SECRET_NAME}[A-Za-z0-9_.-]*\\?["']?\s*(?::=|=>|=|:)\s*)(?P<q>\\?["'])(?P<value>[^"'\r\n\\]+)"#
            ),
            "${1}${q}[secret]",
            quoted_value,
        ),
        // Bare values given to secret names: `password: hunter22`,
        // `?api_key=abc123def`.
        tested(
            format!(
                r#"(?i)(\b[A-Za-z0-9_.-]*?{SECRET_NAME}[A-Za-z0-9_.-]*\s*(?::=|=>|=|:)[ \t]*)(?P<value>[^\s"'`,;()\[\]{{}}<>&]+)"#
            ),
            "${1}[secret]",
            bare_value,
        ),
    ]
});

/// The rules compiled, and a set of them all to see in one pass which can
/// match at all. Word boundaries are ASCII ones: every secret's form is
/// ASCII, and a Unicode boundary in text that isn't (a curly quote, a
/// dash) sends the matcher down its slowest path, many times slower. Next
/// to a letter that isn't ASCII, an ASCII boundary matches where a Unicode
/// one wouldn't, so it only ever finds more.
static COMPILED: LazyLock<(RegexSet, Vec<Regex>)> = LazyLock::new(|| {
    let patterns: Vec<String> = RULES
        .iter()
        .map(|r| r.pattern.replace(r"\b", r"(?-u:\b)"))
        .collect();
    #[allow(clippy::expect_used)]
    let set = RegexSet::new(&patterns).expect("fixed patterns compile");
    (set, patterns.iter().map(|p| re(p)).collect())
});

/// Text with every secret it holds replaced by `[secret]`.
pub fn redact(text: &str) -> String {
    let (set, regexes) = &*COMPILED;
    let hits = set.matches(text);
    if !hits.matched_any() {
        return text.to_string();
    }
    let mut out = text.to_string();
    for i in hits.iter() {
        let (rule, regex) = (&RULES[i], &regexes[i]);
        let next = regex.replace_all(&out, |caps: &Captures| {
            let whole = caps.get(0).map_or("", |m| m.as_str());
            if let Some(test) = rule.test {
                let value = caps.name("value").map_or(whole, |m| m.as_str());
                let after = caps
                    .get(0)
                    .and_then(|m| out.get(m.end()..))
                    .and_then(|rest| rest.chars().next());
                if !test(value, after) {
                    return whole.to_string();
                }
            }
            let mut replaced = String::new();
            caps.expand(rule.replace, &mut replaced);
            replaced
        });
        out = next.into_owned();
    }
    out
}

/// Text with every secret it holds masked by `*`, one for each character,
/// so every line keeps its number and everything else its place: what a
/// file's marks are worked out on. What `redact` would keep of a find (the
/// name before an `=`, the `@` after a password) stays; line breaks inside
/// a secret stay too.
pub fn mask(text: &str) -> String {
    let (set, regexes) = &*COMPILED;
    let hits = set.matches(text);
    if !hits.matched_any() {
        return text.to_string();
    }
    let mut out = text.to_string();
    for i in hits.iter() {
        let (rule, regex) = (&RULES[i], &regexes[i]);
        let keeps_first = rule.replace.contains("${1}");
        let tail = rule.replace.split(SECRET).nth(1).filter(|t| !t.is_empty());
        let mut hidden: Vec<(usize, usize)> = Vec::new();
        for caps in regex.captures_iter(&out) {
            let Some(whole) = caps.get(0) else { continue };
            if let Some(test) = rule.test {
                let value = caps.name("value").map_or(whole.as_str(), |m| m.as_str());
                let after = out.get(whole.end()..).and_then(|rest| rest.chars().next());
                if !test(value, after) {
                    continue;
                }
            }
            let mut start = whole.start();
            let mut end = whole.end();
            if keeps_first && let Some(first) = caps.get(1) {
                start = start.max(first.end());
            }
            if let Some(tail) = tail
                && whole.as_str().ends_with(tail)
            {
                end -= tail.len();
            }
            if start < end {
                hidden.push((start, end));
            }
        }
        if hidden.is_empty() {
            continue;
        }
        let mut next = String::with_capacity(out.len());
        let mut at = 0;
        for (start, end) in hidden {
            next.push_str(&out[at..start]);
            next.extend(
                out[start..end]
                    .chars()
                    .map(|c| if matches!(c, '\n' | '\r') { c } else { '*' }),
            );
            at = end;
        }
        next.push_str(&out[at..]);
        out = next;
    }
    out
}

/// A JSON value with its secrets taken out: every string redacted, and
/// whatever sits under a name that sounds secret (`password`, `apiKey`,
/// `Authorization`, `credentials`) replaced whole. Counts such as
/// `max_tokens` and `input_tokens` are kept.
pub fn redact_value(value: &Value) -> Value {
    match value {
        Value::String(s) => Value::String(redact(s)),
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let v = match v {
                        Value::Null | Value::Bool(_) => v.clone(),
                        Value::String(s) if s.is_empty() => v.clone(),
                        _ if secret_key(k) => Value::String(SECRET.to_string()),
                        _ => redact_value(v),
                    };
                    (redact(k), v)
                })
                .collect(),
        ),
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
    }
}

/// Whether a JSON key names a secret.
fn secret_key(key: &str) -> bool {
    let k: String = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let counts = ["count", "limit", "usage", "budget", "tokeniz", "length"];
    if k.ends_with("tokens")
        || k.starts_with("max")
        || k.starts_with("num")
        || counts.iter().any(|w| k.contains(w))
    {
        return false;
    }
    let secret = [
        "password",
        "passwd",
        "passphrase",
        "secret",
        "token",
        "apikey",
        "accesskey",
        "privatekey",
        "authorization",
        "cookie",
        "credential",
    ];
    secret.iter().any(|w| k.contains(w)) || k == "pwd" || k == "auth"
}

/// Text cut to at most `max_bytes`, keeping its start and its end with
/// `\n…\n` between them. Never cuts inside a character.
pub fn head_tail(text: &str, max_bytes: usize) -> String {
    const CUT: &str = "\n…\n";
    if text.len() <= max_bytes {
        return text.to_string();
    }
    if max_bytes <= CUT.len() {
        return text[..floor_boundary(text, max_bytes)].to_string();
    }
    let room = max_bytes - CUT.len();
    let head = floor_boundary(text, room / 2);
    let tail = ceil_boundary(text, text.len() - (room - head));
    format!("{}{CUT}{}", &text[..head], &text[tail..])
}

/// The largest character boundary at or before `at`.
fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The smallest character boundary at or after `at`.
fn ceil_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at += 1;
    }
    at
}

static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}\b"));
static PHONE: LazyLock<Regex> = LazyLock::new(|| re(r"\+?\(?\d[\d \t().-]{6,}\d"));
static LINK: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(?:[a-z][a-z0-9+.-]{1,20}://|www\.[a-z0-9-]+\.|(?:mailto|javascript|data):)|[\w-]+(?:\.[\w-]+)+/\S",
    )
});

/// Text with other people's email addresses and phone numbers taken out.
/// Names stay: entries are about a person's life, and need its people.
pub fn other_people(text: &str) -> String {
    let text = EMAIL.replace_all(text, |caps: &Captures| {
        let found = caps.get(0).map_or("", |m| m.as_str());
        if is_email(found) {
            "[email]".to_string()
        } else {
            found.to_string()
        }
    });
    PHONE
        .replace_all(&text, |caps: &Captures| {
            let found = caps.get(0).map_or("", |m| m.as_str());
            if is_phone(found) {
                "[phone]".to_string()
            } else {
                found.to_string()
            }
        })
        .into_owned()
}

/// Whether an email-shaped find is an address, not `git@github.com` or
/// `icon@2x.png`.
fn is_email(found: &str) -> bool {
    let Some((local, domain)) = found.split_once('@') else {
        return false;
    };
    let scale =
        domain.len() > 2 && domain.as_bytes()[0].is_ascii_digit() && domain[1..].starts_with("x.");
    local != "git" && !scale
}

/// Whether a run of digits and separators is a phone number: 9 to 15
/// digits, some separator or a leading `+`, at least one group of three or
/// more and none past six, and not years or an IP address.
fn is_phone(found: &str) -> bool {
    let digits = found.chars().filter(char::is_ascii_digit).count();
    if !(9..=15).contains(&digits) {
        return false;
    }
    let groups: Vec<&str> = found
        .split(|c: char| !c.is_ascii_digit())
        .filter(|g| !g.is_empty())
        .collect();
    if groups.len() < 2 && !found.starts_with('+') {
        return false;
    }
    if groups.len() > 5
        || groups.iter().all(|g| g.len() < 3)
        || (groups.len() > 1 && groups.iter().any(|g| g.len() > 6))
    {
        return false;
    }
    let years = groups
        .iter()
        .all(|g| g.len() == 4 && (g.starts_with("19") || g.starts_with("20")));
    let dotted = !found.contains([' ', '-', '(', ')', '\t']);
    let ip = dotted && groups.len() == 4 && groups.iter().all(|g| g.len() <= 3);
    !years && !ip
}

/// Why a search query can't be sent.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum QueryProblem {
    #[error("The query is too short to search for.")]
    TooShort,
    #[error("The query is longer than 200 characters.")]
    TooLong,
    #[error("The query has more than 20 words.")]
    TooManyWords,
    #[error("The query has something in it that looks like a secret.")]
    Secret,
    #[error("The query has an email address or a phone number in it.")]
    Contact,
    #[error("The query has a link in it.")]
    Link,
    #[error(
        "The query has a long encoded-looking string in it, which may have come from activity."
    )]
    Encoded,
    #[error("The query mentions #{0}, a sensitive topic.")]
    Sensitive(String),
}

/// Whether a search query may leave the computer: no secret, no email
/// address or phone number, no link, no long encoded-looking string, 2 to
/// 200 characters and at most 20 words, and none of the `sensitive` topics'
/// names.
pub fn query_ok(query: &str, sensitive: &[&str]) -> Result<(), QueryProblem> {
    let query = query.trim();
    let chars = query.chars().count();
    if chars < 2 {
        return Err(QueryProblem::TooShort);
    }
    if chars > 200 {
        return Err(QueryProblem::TooLong);
    }
    if query.split_whitespace().count() > 20 {
        return Err(QueryProblem::TooManyWords);
    }
    if redact(query) != query {
        return Err(QueryProblem::Secret);
    }
    if other_people(query) != query {
        return Err(QueryProblem::Contact);
    }
    if LINK.is_match(query) {
        return Err(QueryProblem::Link);
    }
    if query.split_whitespace().any(encoded) {
        return Err(QueryProblem::Encoded);
    }
    let words = words_of(query);
    for name in sensitive {
        let name = words_of(name);
        if !name.is_empty() && format!(" {words} ").contains(&format!(" {name} ")) {
            return Err(QueryProblem::Sensitive(name.replace(' ', "-")));
        }
    }
    Ok(())
}

/// Whether a word has a long run of what base64, hex or an id is made of.
/// A long hyphenated phrase without digits is still words.
fn encoded(word: &str) -> bool {
    let mut run = String::new();
    let mut runs = Vec::new();
    for c in word.chars() {
        if c.is_ascii_alphanumeric() || "+/=_-".contains(c) {
            run.push(c);
        } else if !run.is_empty() {
            runs.push(std::mem::take(&mut run));
        }
    }
    runs.push(run);
    runs.iter().any(|run| {
        run.len() >= 24
            && (!run.contains('-')
                || run.chars().filter(char::is_ascii_digit).count() >= 4
                || run.contains(['+', '/', '=']))
    })
}

/// Text lowercased, with everything but letters and digits as single
/// spaces, for matching whole words.
fn words_of(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

// The tests a find must pass under the rules that could catch ordinary
// code. Each takes the found value and the character after the find.

/// Not a stand-in for a secret: `$TOKEN`, `${KEY}`, `<your key>`, `***`,
/// `[secret]`.
fn placeholder(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    value.is_empty()
        || value.starts_with(['$', '<', '%', '[', '{'])
        || value
            .chars()
            .all(|c| matches!(c, '*' | 'x' | 'X' | '.' | '…' | '-' | '_'))
        || lower.starts_with("your")
        || lower.contains("redacted")
        || matches!(
            lower.as_str(),
            "true" | "false" | "null" | "none" | "nil" | "undefined" | "required" | "optional"
        )
}

fn not_placeholder(value: &str, _after: Option<char>) -> bool {
    !placeholder(value)
}

/// Looks like a key, not a word: a digit, or both cases.
fn random_looking(value: &str, _after: Option<char>) -> bool {
    let digit = value.chars().any(|c| c.is_ascii_digit());
    let upper = value.chars().any(|c| c.is_ascii_uppercase());
    let lower = value.chars().any(|c| c.is_ascii_lowercase());
    !placeholder(value) && (digit || (upper && lower))
}

/// A cookie's `name=value`: a name before the `=`, so code that names a
/// cookie (`COOKIE: &str = "kelve"`) isn't taken for one.
fn cookie(value: &str, _after: Option<char>) -> bool {
    value.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    })
}

fn numeric(value: &str) -> bool {
    value
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '_'))
}

/// In a `.env` line, anything but a stand-in or a number is a secret.
fn env_value(value: &str, _after: Option<char>) -> bool {
    !placeholder(value) && !numeric(value)
}

/// A quoted value: anything but a stand-in, a number, a single letter, a
/// sentence or a setting.
fn quoted_value(value: &str, _after: Option<char>) -> bool {
    !placeholder(value)
        && !numeric(value)
        && value.chars().count() >= 3
        && !sentence(value)
        && !SETTINGS.contains(&value.to_ascii_lowercase().as_str())
}

/// A sentence, as messages and definitions are: four words or more, ending
/// as a sentence ends. `Skip::Secret => "kelve doesn't read this file."`
fn sentence(value: &str) -> bool {
    value.split_whitespace().count() >= 4 && value.trim_end().ends_with(['.', '?', '!', ':'])
}

/// Values a secret-sounding name takes as a setting, not a secret:
/// `credentials: 'same-origin'`.
const SETTINGS: [&str; 6] = [
    "include",
    "omit",
    "same-origin",
    "enabled",
    "disabled",
    "off",
];

/// A bare value, which is where code lives: not a call, a name, a path
/// through names (`self.token`, `Token::new`) or a type, and long or mixed
/// enough to be a secret.
fn bare_value(value: &str, after: Option<char>) -> bool {
    if placeholder(value) || numeric(value) || matches!(after, Some('(' | '<' | '[' | '{' | '.')) {
        return false;
    }
    let name = |part: &str| {
        part.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    let words_only = value.chars().all(|c| c.is_ascii_alphabetic() || c == '_');
    let path = value.contains("::") || (value.contains('.') && value.split('.').all(name));
    if words_only || path || value.starts_with('&') {
        return false;
    }
    let length = value.chars().count();
    let digit = value.chars().any(|c| c.is_ascii_digit());
    let symbol = value
        .chars()
        .any(|c| !c.is_ascii_alphanumeric() && c != '_');
    let mixed = value.chars().any(|c| c.is_ascii_uppercase())
        && value.chars().any(|c| c.is_ascii_lowercase());
    length >= 20 || (length >= 6 && (digit || symbol || mixed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn gone(text: &str, secret: &str) {
        let out = redact(text);
        assert!(!out.contains(secret), "kept {secret:?} in {out:?}");
        assert!(out.contains(SECRET), "{out:?}");
    }

    fn kept(text: &str) {
        assert_eq!(redact(text), text);
    }

    /// Masked text has the same characters, lines and places, and none of
    /// what `redact` takes out.
    fn masked(text: &str, secret: &str) -> String {
        let out = mask(text);
        assert_eq!(out.chars().count(), text.chars().count(), "{out:?}");
        assert_eq!(out.lines().count(), text.lines().count(), "{out:?}");
        assert!(!out.contains(secret), "kept {secret:?} in {out:?}");
        out
    }

    #[test]
    fn masking_keeps_every_place() {
        let out = masked(
            "let a = 1;\nconst KEY = \"sk-ant-api03-AbCdEf0123456789_AbCdEf0123456789-xyz\";\nlet b = 2;",
            "sk-ant-api03-AbCdEf0123456789",
        );
        assert!(out.starts_with("let a = 1;\n"), "{out:?}");
        assert!(out.ends_with("\nlet b = 2;"), "{out:?}");
        // What redact keeps of a find stays: the name, and the @ after a password.
        let out = masked("password = hunter2-Secret!", "hunter2");
        assert!(out.starts_with("password = "), "{out:?}");
        let out = masked("postgres://kelve:s3cret-pass@localhost/db", "s3cret");
        assert!(out.contains("@localhost/db"), "{out:?}");
        // A private key keeps its lines, each masked.
        let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQ\nAAAAMwAAAAtzc2gtZW\n-----END OPENSSH PRIVATE KEY-----";
        let out = masked(
            &format!("a\n{key}\nb"),
            "b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQ",
        );
        assert_eq!(out.lines().count(), 6);
        // Ordinary code passes through untouched.
        let code = "let token = parse(input);\nmax_tokens: 4096";
        assert_eq!(mask(code), code);
    }

    #[test]
    fn known_tokens_go() {
        let cases = [
            "ghp_0123456789abcdefghijABCDEFGHIJ012345",
            "gho_0123456789abcdefghijABCDEFGHIJ012345",
            "ghu_0123456789abcdefghijABCDEFGHIJ012345",
            "ghs_0123456789abcdefghijABCDEFGHIJ012345",
            "ghr_0123456789abcdefghijABCDEFGHIJ012345",
            "github_pat_11ABCDEFG0123456789_abcdefghijklmnopqrstuvwxyz",
            "glpat-abcdefghij0123456789",
            "xoxb-1234567890-0987654321-AbCdEfGhIjKl",
            "xoxp-1234567890-0987654321-AbCdEfGhIjKl",
            "https://hooks.slack.com/services/T000/B000/XXXXXXXXXXXXXXXX",
            "https://discord.com/api/webhooks/123456789/abcDEF_ghi-jkl",
            "sk_live_51H8abcdefghijklmnop",
            "rk_live_51H8abcdefghijklmnop",
            "pk_live_51H8abcdefghijklmnop",
            "sk-proj-AbCdEf0123456789AbCdEf0123456789",
            "sk-0123456789abcdefABCDEF0123456789",
            "sk-ant-api03-AbCdEf0123456789_AbCdEf0123456789-xyz",
            "sk-or-v1-0123456789abcdef0123456789abcdef0123456789abcdef",
            "AIzaSyA-0123456789abcdefghijklmnopqrstu",
            "GOCSPX-AbCdEf0123456789AbCdEf0123",
            "ya29.a0AfH6SMBx0123456789abcdefghij",
            "BSAv1neIuQOsxqOyy0sEe_ie2zD_n_V",
            "hf_AbCdEfGhIjKlMnOpQrStUvWxYz01234567",
            "npm_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
            "SG.AbCdEfGhIjKlMnOpQrStUv.AbCdEfGhIjKlMnOpQrStUvWxYz0123456789abcdefg",
            "key-0123456789abcdef0123456789abcdef",
            "SK0123456789abcdef0123456789abcdef",
            "MTA4NjQ1MjM0NTY3ODkwMTIz.GaBcDe.AbCdEfGhIjKlMnOpQrStUvWxYz01234",
            "123456789:AAAbCdEfGhIjKlMnOpQrStUvWxYz0123456",
            "AKIAIOSFODNN7EXAMPLE",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
        ];
        for secret in cases {
            gone(&format!("the key is {secret} ok"), secret);
            // Beside text that isn't ASCII, too: a curly quote, a dash, an
            // accented letter.
            gone(&format!("the key is “{secret}” — ok"), secret);
            gone(&format!("clé:{secret}é"), secret);
        }
    }

    #[test]
    fn long_text_that_isnt_ascii_is_quick() {
        // A transcript's worth of curly quotes and dashes: the matcher stays
        // on its fast path.
        let text = "It’s the bind mount — it hides what’s there. ".repeat(20_000);
        let started = std::time::Instant::now();
        assert_eq!(redact(&text), text);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn private_keys_go_whole_or_cut() {
        let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQ\nAAAAMwAAAAtzc2gtZW\n-----END OPENSSH PRIVATE KEY-----";
        let out = redact(&format!("before\n{key}\nafter"));
        assert_eq!(out, "before\n[secret]\nafter");
        let cut =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA0123456789abcdef\nAbCdEf0123+/=";
        let out = redact(&format!("{cut}\n…\nlast line of the output"));
        assert!(!out.contains("MIIEow"), "{out}");
        assert!(out.contains("last line of the output"), "{out}");
        // Cut off with nothing after it but prose, which stays.
        let out = redact(&format!("{cut}\nthe rest of the output"));
        assert_eq!(out, "[secret]\nthe rest of the output");
        // Inside a JSON string, with its line breaks escaped.
        let json = r#"{"content":"-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASC\nBKcwggSjAgEAAoIBAQC7\n"}"#;
        let out = redact(json);
        assert!(!out.contains("MIIEvQ") && !out.contains("BKcwgg"), "{out}");
        let encrypted = "-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC,0123456789ABCDEF\n\nMIIEowIBAAKCAQEA0123456789abcdef";
        let out = redact(encrypted);
        assert_eq!(out, "[secret]");
    }

    #[test]
    fn assignments_in_every_form_go() {
        gone("DATABASE_PASSWORD=hunter2", "hunter2");
        gone("export GITHUB_TOKEN=\"abc123def456\"", "abc123def456");
        gone("OPENAI_API_KEY=correcthorsebattery", "correcthorsebattery");
        gone(r#"{"api_key": "abcdEFGH1234"}"#, "abcdEFGH1234");
        gone("client_secret: 'zyx987wvu654'", "zyx987wvu654");
        gone("password = \"correct horse\"", "correct");
        gone("db:\n  password: Tr0ub4dor&3\n", "Tr0ub4dor");
        gone(
            "aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "wJalrXUtnFEMI",
        );
        gone(
            "https://api.example.com/v1?api_key=abc123def456&page=2",
            "abc123def456",
        );
        gone("mysql --password hunter22 -h db", "hunter22");
        gone("tool --token=Zm9vYmFyYmF6 run", "Zm9vYmFyYmF6");
        gone(
            "//registry.npmjs.org/:_authToken=npm_secretvalue123",
            "npm_secretvalue123",
        );
        gone(
            "Server=db;Database=app;User Id=sa;Password=P@ssw0rd!;",
            "P@ssw0rd",
        );
        gone(
            "DefaultEndpointsProtocol=https;AccountName=x;AccountKey=AbCdEf0123456789AbCdEf0123456789==;",
            "AbCdEf0123456789",
        );
        gone(
            "TWILIO_AUTH_TOKEN=0123456789abcdef0123456789abcdef",
            "0123456789abcdef",
        );
    }

    #[test]
    fn headers_and_addresses_go() {
        gone(
            "Authorization: Bearer abc.def.ghi-0123456789xyz",
            "0123456789",
        );
        gone("authorization: Basic dXNlcjpwYXNz", "dXNlcjpwYXNz");
        gone("-H 'X-Subscription-Token: BSAabc123'", "BSAabc123");
        gone("x-api-key: 0123abcd4567", "0123abcd4567");
        gone("Cookie: session=AbC123xyz; theme=dark", "AbC123xyz");
        gone("curl -u admin:s3cret https://x", "s3cret");
        let out = redact("DATABASE_URL=postgres://kelve:hunter2@localhost:5442/kelve");
        assert!(!out.contains("hunter2"), "{out}");
        let out = redact("connect to mongodb+srv://me:pa55word@cluster0.example.net/db");
        assert_eq!(
            out,
            "connect to mongodb+srv://[secret]@cluster0.example.net/db"
        );
        let out = redact("redis://:onlypassword@cache:6379");
        assert!(!out.contains("onlypassword"), "{out}");
    }

    #[test]
    fn secrets_in_odd_places_go() {
        // Inside a JSON string inside a log line.
        gone(
            r#"error: {"config":{"stripe":"sk_live_51H8abcdefghijklmnop"}}"#,
            "sk_live_51H8",
        );
        // In a command an agent ran.
        gone(
            "git clone https://x-access-token:ghs_abcdefghijABCDEFGHIJ0123@github.com/a/b",
            "ghs_abc",
        );
        // Twice on one line, and split by quotes.
        let out = redact("A=ghp_0123456789abcdefghijABCD B='ghp_abcdefghij0123456789WXYZ'");
        assert_eq!(out.matches(SECRET).count(), 2, "{out}");
        // In camelCase code.
        gone(
            "const stripeSecretKey = 'rk_test_abcdefghij0123';",
            "abcdefghij0123",
        );
        // Escaped JSON in JSON.
        gone(r#""{\"password\": \"hunter22\"}""#, "hunter22");
    }

    #[test]
    fn ordinary_code_and_prose_survive() {
        let cases = [
            "let token = parse(input);",
            "fn check(token: &str) -> bool {",
            "pub api_key: Option<String>,",
            "max_tokens: 4096",
            "\"input_tokens\": 1234, \"output_tokens\": 99",
            "MAX_TOKENS=4096",
            "TOKENIZERS_PARALLELISM=false",
            "self.token = token.clone();",
            "password: String,",
            "token = self.next_token()",
            "let secret = Secret::new(bytes);",
            "secret_key = config.secret_key",
            "The token was refused, so the agent asked for a new one.",
            "Passwords must be at least 12 characters.",
            "Use a bearer token in the Authorization header.",
            "git push -u origin main",
            "The risk-assessment-for-the-quarterly-plan is due.",
            "export PATH=$HOME/.cargo/bin:$PATH",
            "GITHUB_TOKEN=${{ secrets.GITHUB_TOKEN }}",
            "token: $TOKEN",
            "api_key: <your key here>",
            "password: ********",
            "https://docs.docker.com/engine/storage/volumes/",
            "git@github.com:acme/app.git",
            "Docker makes folders as root.",
            "use std::sync::LazyLock;",
            "if token == expected { return; }",
            "\"type\": \"password\"",
            "pub const COOKIE: &str = \"kelve\";",
            "fetch(path, { credentials: 'same-origin', ...init })",
            "Skip::Secret => \"kelve doesn’t read this file: it may hold secrets.\",",
            "\"Secret\": \"Something only those meant to know it know.\",",
        ];
        for text in cases {
            kept(text);
        }
    }

    #[test]
    fn secrets_where_settings_and_sentences_sit_still_go() {
        for (text, gone) in [
            (
                "credentials: 'sk-live-0123456789abcdef'",
                "sk-live-0123456789abcdef",
            ),
            (
                "\"secret\": \"correct horse battery staple\"",
                "correct horse battery staple",
            ),
            ("Cookie: session=0123456789abcdef", "0123456789abcdef"),
        ] {
            assert!(!mask(text).contains(gone), "{text} → {}", mask(text));
        }
    }

    #[test]
    fn redacting_twice_changes_nothing() {
        let text =
            "PASSWORD=hunter22 and ghp_0123456789abcdefghijABCD and \"token\": \"abc123def\"";
        let once = redact(text);
        assert_eq!(redact(&once), once);
    }

    #[test]
    fn values_under_secret_names_go_whole() {
        let value = json!({
            "session_id": "abc",
            "tool_input": {
                "command": "curl -H 'Authorization: Bearer sk-proj-AbCdEf0123456789AbCdEf0123'",
                "env": {"API_KEY": "plainword", "credentials": {"user": "me", "pass": "x"}},
                "headers": [{"Cookie": "a=b"}],
            },
            "usage": {"input_tokens": 12, "max_tokens": 4096, "token_count": 5},
            "secret": false,
            "apiKey": 12345,
        });
        let out = redact_value(&value);
        assert_eq!(out["session_id"], "abc");
        assert_eq!(out["tool_input"]["env"]["API_KEY"], SECRET);
        assert_eq!(out["tool_input"]["env"]["credentials"], SECRET);
        assert_eq!(out["tool_input"]["headers"][0]["Cookie"], SECRET);
        assert_eq!(out["usage"]["input_tokens"], 12);
        assert_eq!(out["usage"]["max_tokens"], 4096);
        assert_eq!(out["usage"]["token_count"], 5);
        assert_eq!(out["secret"], false);
        assert_eq!(out["apiKey"], SECRET);
        let command = out["tool_input"]["command"].as_str().unwrap_or_default();
        assert!(!command.contains("AbCdEf0123"), "{command}");
    }

    #[test]
    fn head_tail_keeps_both_ends() {
        assert_eq!(head_tail("short", 100), "short");
        let text = format!("{}{}", "a".repeat(100), "z".repeat(100));
        let out = head_tail(&text, 50);
        assert!(out.len() <= 50, "{}", out.len());
        assert!(out.starts_with("aaa") && out.ends_with("zzz"));
        assert!(out.contains("\n…\n"));
        // Never inside a character.
        let text = "é".repeat(100);
        for max in 0..60 {
            let out = head_tail(&text, max);
            assert!(out.len() <= max.max(1), "{max}: {}", out.len());
        }
        let out = head_tail("😀😀😀😀😀😀😀😀😀😀", 20);
        assert!(out.len() <= 20 && out.contains('…'), "{out}");
    }

    #[test]
    fn other_people_lose_their_contact_details() {
        assert_eq!(
            other_people("Ask Sam at sam.jones@example.co.uk or +44 20 7946 0958."),
            "Ask Sam at [email] or [phone]."
        );
        assert_eq!(
            other_people("Call (555) 123-4567 today"),
            "Call [phone] today"
        );
        for text in [
            "git@github.com:acme/app.git",
            "icon@2x.png",
            "between 2019-2024 and 2025",
            "the server at 192.168.100.200",
            "on 2026-10-12 at 14:30",
            "1,108 errors and 1728734400000 ms",
            "scores 10 20 30 40 50",
            "@types/node and node@20",
        ] {
            assert_eq!(other_people(text), text);
        }
    }

    #[test]
    fn good_queries_go() {
        for query in [
            "docker named volume ownership",
            "Cohen's kappa interpretation",
            "LLM judge length bias 2024",
            "node.js ESM top-level side effects",
            "state-of-the-art-machine-translation evaluation",
        ] {
            assert_eq!(query_ok(query, &["health", "money"]), Ok(()), "{query}");
        }
    }

    #[test]
    fn bad_queries_stay() {
        use QueryProblem as Q;
        assert_eq!(query_ok("a", &[]), Err(Q::TooShort));
        assert_eq!(query_ok("   ", &[]), Err(Q::TooShort));
        assert_eq!(query_ok(&"word ".repeat(50), &[]), Err(Q::TooLong));
        assert_eq!(query_ok(&"w ".repeat(21), &[]), Err(Q::TooManyWords));
        assert_eq!(
            query_ok("ghp_0123456789abcdefghijABCD docs", &[]),
            Err(Q::Secret)
        );
        assert_eq!(
            query_ok("email sam@example.com about it", &[]),
            Err(Q::Contact)
        );
        assert_eq!(query_ok("call +1 555 123 4567", &[]), Err(Q::Contact));
        assert_eq!(
            query_ok("see https://evil.example/x?q=1", &[]),
            Err(Q::Link)
        );
        assert_eq!(query_ok("www.example.com pricing", &[]), Err(Q::Link));
        assert_eq!(query_ok("evil.example/collect data", &[]), Err(Q::Link));
        assert_eq!(
            query_ok("what is dGhpcyBpcyBhIHNlY3JldCBtZXNzYWdl", &[]),
            Err(Q::Encoded)
        );
        assert_eq!(
            query_ok("commit 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b", &[]),
            Err(Q::Encoded)
        );
        assert_eq!(
            query_ok("550e8400-e29b-41d4-a716-446655440000 error", &[]),
            Err(Q::Encoded)
        );
        assert_eq!(
            query_ok("Mental health and remote work", &["mental-health"]),
            Err(Q::Sensitive("mental-health".into()))
        );
        // A word inside a longer word isn't the topic.
        assert_eq!(query_ok("docker healthcheck interval", &["health"]), Ok(()));
    }
}
