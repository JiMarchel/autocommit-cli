//! Treat model output as untrusted: sanitize, validate, and deterministically repair.

use crate::hints::{Hints, sanitize_scope};
use std::fmt;

pub const TYPES: &[&str] = &[
    "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert",
];
pub const MAX_SUBJECT: usize = 72;
pub const MAX_BULLETS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    Empty,
    BadHeader,
    UnknownType(String),
    WrongType {
        expected: &'static str,
        found: String,
    },
    SubjectTooLong(usize),
    TrailingPeriod,
    UppercaseStart,
    BadBody,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Violation::Empty => write!(f, "the message is empty"),
            Violation::BadHeader => write!(
                f,
                "the first line must look like \"type(scope): description\" or \"type: description\""
            ),
            Violation::UnknownType(t) => write!(
                f,
                "\"{t}\" is not an allowed type; use one of: {}",
                TYPES.join(", ")
            ),
            Violation::WrongType { expected, found } => {
                write!(f, "the type must be \"{expected}\", not \"{found}\"")
            }
            Violation::SubjectTooLong(n) => write!(
                f,
                "the first line has {n} characters; it must have at most {MAX_SUBJECT}"
            ),
            Violation::TrailingPeriod => write!(f, "the first line must not end with a period"),
            Violation::UppercaseStart => {
                write!(f, "the description must start with a lowercase letter")
            }
            Violation::BadBody => write!(
                f,
                "after the first line there must be either nothing, or a blank line followed by at most {MAX_BULLETS} lines starting with \"- \""
            ),
        }
    }
}

/// Loosely parsed header: `type(scope)!: description`.
struct Header<'a> {
    ty: &'a str,
    scope: Option<&'a str>,
    bang: bool,
    /// Text after the colon, NOT trimmed.
    rest: &'a str,
}

fn parse_header(line: &str) -> Option<Header<'_>> {
    let ty_end = line
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(line.len());
    if ty_end == 0 {
        return None;
    }
    let ty = &line[..ty_end];
    let mut rest = &line[ty_end..];
    let mut scope = None;
    if let Some(r) = rest.strip_prefix('(') {
        let close = r.find(')')?;
        scope = Some(&r[..close]);
        rest = &r[close + 1..];
    }
    let bang = rest.starts_with('!');
    if bang {
        rest = &rest[1..];
    }
    let rest = rest.strip_prefix(':')?;
    Some(Header {
        ty,
        scope,
        bang,
        rest,
    })
}

fn valid_scope(scope: &str) -> bool {
    !scope.is_empty()
        && scope.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '/' | '-')
        })
}

/// "Add ..." is wrong; "README ..." / "GitHub ..." (acronyms, proper nouns) are fine.
fn starts_with_capitalized_word(desc: &str) -> bool {
    let word = desc.split_whitespace().next().unwrap_or("");
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => {
            let rest = chars.as_str();
            !rest.is_empty() && !rest.chars().any(char::is_uppercase)
        }
        _ => false,
    }
}

fn lowercase_first(desc: &str) -> String {
    let mut chars = desc.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

const LABELS: &[&str] = &["commit message:", "commit msg:", "message:", "commit:"];
const WRAPPERS: &[&str] = &["**", "__", "\"", "'", "`"];

/// Strip the typical wrapping a chatty model adds, and normalize whitespace.
pub fn sanitize(raw: &str) -> String {
    let mut text = raw
        .lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    // Preamble lines / labels.
    loop {
        let first = text.lines().next().unwrap_or("").trim();
        let lower = first.to_ascii_lowercase();
        let remainder = text.split_once('\n').map(|(_, r)| r).unwrap_or("");
        if let Some(label) = LABELS.iter().find(|l| lower.starts_with(**l)) {
            let head = first[label.len()..].trim();
            text = if head.is_empty() {
                remainder.trim().to_string()
            } else {
                format!("{head}\n{remainder}").trim().to_string()
            };
        } else if lower.ends_with(':') && (lower.starts_with("here") || lower.contains("commit")) {
            text = remainder.trim().to_string();
        } else {
            break;
        }
    }

    // Markdown heading + wrappers around the whole message.
    text = text.trim_start_matches('#').trim().to_string();
    loop {
        let before = text.len();
        for w in WRAPPERS {
            if text.len() >= 2 * w.len() && text.starts_with(w) && text.ends_with(w) {
                text = text[w.len()..text.len() - w.len()].trim().to_string();
            }
        }
        if text.len() == before {
            break;
        }
    }

    // Whitespace: header, blank line, body with collapsed blank runs.
    let mut lines = text.lines().map(str::trim);
    let header = lines.next().unwrap_or("").to_string();
    let mut body: Vec<&str> = Vec::new();
    for line in lines {
        if line.is_empty() && body.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        body.push(line);
    }
    while body.last().is_some_and(|l| l.is_empty()) {
        body.pop();
    }
    if body.is_empty() {
        header
    } else {
        format!("{header}\n\n{}", body.join("\n"))
    }
}

pub fn validate(msg: &str, locked: Option<&str>) -> Result<(), Violation> {
    let msg = msg.trim();
    if msg.is_empty() {
        return Err(Violation::Empty);
    }
    let mut lines = msg.lines();
    let first = lines.next().unwrap_or("");
    let h = parse_header(first).ok_or(Violation::BadHeader)?;
    if h.scope.is_some_and(|s| !valid_scope(s)) {
        return Err(Violation::BadHeader);
    }
    let desc = h.rest.strip_prefix(' ').ok_or(Violation::BadHeader)?;
    if desc.is_empty() || desc.starts_with(char::is_whitespace) {
        return Err(Violation::BadHeader);
    }
    if !TYPES.contains(&h.ty) {
        return Err(Violation::UnknownType(h.ty.to_string()));
    }
    if let Some(expected) = locked
        && h.ty != expected
    {
        let expected = TYPES
            .iter()
            .copied()
            .find(|t| *t == expected)
            .unwrap_or("chore");
        return Err(Violation::WrongType {
            expected,
            found: h.ty.to_string(),
        });
    }
    let len = first.chars().count();
    if len > MAX_SUBJECT {
        return Err(Violation::SubjectTooLong(len));
    }
    if first.ends_with('.') {
        return Err(Violation::TrailingPeriod);
    }
    if starts_with_capitalized_word(desc) {
        return Err(Violation::UppercaseStart);
    }

    let body: Vec<&str> = lines.collect();
    if !body.is_empty() {
        if !body[0].trim().is_empty() {
            return Err(Violation::BadBody);
        }
        let content: Vec<&str> = body
            .iter()
            .copied()
            .filter(|l| !l.trim().is_empty())
            .collect();
        if content.len() > MAX_BULLETS
            || content
                .iter()
                .any(|l| !l.starts_with("- ") || l[2..].trim().is_empty())
        {
            return Err(Violation::BadBody);
        }
    }
    Ok(())
}

/// Replace the type of a parseable header, leaving everything else untouched.
pub fn enforce_type(msg: &str, ty: &str) -> String {
    match parse_header(msg) {
        Some(h) => format!("{ty}{}", &msg[h.ty.len()..]),
        None => msg.to_string(),
    }
}

fn normalize_type(raw: &str) -> Option<&'static str> {
    let lower = raw.to_ascii_lowercase();
    let mapped = match lower.as_str() {
        "feature" | "features" | "add" => "feat",
        "bugfix" | "bug" | "hotfix" | "fixes" => "fix",
        "doc" | "documentation" => "docs",
        "tests" | "testing" => "test",
        "refactoring" => "refactor",
        "performance" => "perf",
        "chores" => "chore",
        other => other,
    };
    TYPES.iter().copied().find(|t| *t == mapped)
}

fn clean_desc(desc: &str) -> String {
    let mut d = desc
        .trim()
        .trim_end_matches(|c: char| matches!(c, '.' | '!' | ',' | ';' | ':') || c.is_whitespace())
        .to_string();
    if starts_with_capitalized_word(&d) {
        d = lowercase_first(&d);
    }
    d
}

/// Cut `desc` to at most `budget` chars, preferring a word boundary.
fn truncate_words(desc: &str, budget: usize) -> String {
    if desc.chars().count() <= budget {
        return desc.to_string();
    }
    let cut: String = desc.chars().take(budget).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > 0 => cut[..i].to_string(),
        _ => cut,
    };
    clean_desc(&cut)
}

fn bullet_text(line: &str) -> Option<&str> {
    let line = line.trim();
    ["- ", "* ", "• ", "+ "]
        .iter()
        .find_map(|p| line.strip_prefix(p))
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

/// Deterministically turn anything into a message that passes `validate`.
pub fn repair(msg: &str, hints: &Hints) -> String {
    let msg = msg.trim();
    let (first, body) = msg.split_once('\n').unwrap_or((msg, ""));
    let first = first.trim();

    let parsed = parse_header(first);
    let (parsed_type, mut scope, bang, desc) = match &parsed {
        Some(h) => (
            normalize_type(h.ty),
            h.scope.and_then(sanitize_scope),
            h.bang,
            h.rest,
        ),
        None => (None, hints.scope.clone(), false, first),
    };
    // An unrecognised "type" is more likely a word of the description.
    let desc = if parsed.is_some() && parsed_type.is_none() {
        first
    } else {
        desc
    };

    let ty = hints
        .locked_type
        .or(parsed_type)
        .or(hints.suggested_type)
        .unwrap_or("chore");

    let placeholder = match scope.as_deref().or(hints.scope.as_deref()) {
        Some(s) => format!("update {s}"),
        None => "update files".to_string(),
    };
    let mut desc = clean_desc(desc);
    if !desc.chars().any(char::is_alphanumeric) {
        desc = placeholder.clone();
    }

    let prefix = |scope: &Option<String>| match scope {
        Some(s) => format!("{ty}({s}){}: ", if bang { "!" } else { "" }),
        None => format!("{ty}{}: ", if bang { "!" } else { "" }),
    };
    if prefix(&scope).chars().count() > MAX_SUBJECT / 2 {
        scope = None;
    }
    let prefix = prefix(&scope);
    let budget = MAX_SUBJECT - prefix.chars().count();
    let mut desc = truncate_words(&desc, budget);
    // Truncation can leave only punctuation (e.g. "....fix").
    if !desc.chars().any(char::is_alphanumeric) {
        desc = truncate_words(&placeholder, budget);
    }

    let bullets: Vec<String> = body
        .lines()
        .filter_map(bullet_text)
        .take(MAX_BULLETS)
        .map(|b| format!("- {b}"))
        .collect();

    let header = format!("{prefix}{desc}");
    if bullets.is_empty() {
        header
    } else {
        format!("{header}\n\n{}", bullets.join("\n"))
    }
}

#[cfg(test)]
mod tests;
