//! Prompt construction. Short and strict: the model is weak.

use crate::hints::Hints;
use crate::message::{MAX_BULLETS, MAX_SUBJECT, TYPES, Violation};

pub fn build(hints: &Hints, rendered_diff: &str) -> String {
    let type_line = match (hints.locked_type, hints.suggested_type) {
        (Some(t), _) => format!("Type: MUST be \"{t}\"."),
        (None, Some(t)) => format!("Type: probably \"{t}\", but choose what fits the diff."),
        (None, None) => "Type: choose what fits the diff.".to_string(),
    };
    let scope_line = match &hints.scope {
        Some(s) => format!("Scope: use \"{s}\"."),
        None => "Scope: optional; leave it out if unsure.".to_string(),
    };
    format!(
        "You write git commit messages. Reply with ONLY the commit message: \
no explanation, no markdown, no code fences, no quotes.

Format:
<type>(<scope>): <description>

- optional detail
- optional detail

Rules:
1. <type> is one of: {types}.
2. The first line has at most {MAX_SUBJECT} characters.
3. <description> is in English, imperative mood (\"add\", not \"added\"), \
starts with a lowercase letter, and has no period at the end.
4. The body is optional: a blank line, then at most {MAX_BULLETS} short lines starting with \"- \".
5. Only describe changes that are visible in the diff below. Do not invent anything.

Examples:
feat(parser): support quoted file names
fix: prevent crash when the config file is missing

- fall back to the default config instead of panicking

{type_line}
{scope_line}

{rendered_diff}",
        types = TYPES.join(", "),
    )
}

pub fn correction(base: &str, previous: &str, violation: &Violation) -> String {
    format!(
        "{base}\n\nYour previous answer was:\n{previous}\n\n\
It is invalid because {violation}.\n\
Write the corrected commit message now. Reply with ONLY the commit message."
    )
}

#[cfg(test)]
mod tests;
