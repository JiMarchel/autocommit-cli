//! Deterministic type/scope hints derived from the file list.
//!
//! The model is weak, so anything we can decide from paths alone is decided
//! here: a "locked" type is enforced in code, a "suggested" type is advisory.

use crate::diff::{FileChange, Status, file_name, is_lockfile};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hints {
    /// Type the message MUST use (enforced in code).
    pub locked_type: Option<&'static str>,
    /// Type that is likely correct (advisory only).
    pub suggested_type: Option<&'static str>,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ci,
    Test,
    Chore,
    Docs,
    Code,
}

impl Kind {
    fn locked_type(self) -> Option<&'static str> {
        match self {
            Kind::Ci => Some("ci"),
            Kind::Test => Some("test"),
            Kind::Chore => Some("chore"),
            Kind::Docs => Some("docs"),
            Kind::Code => None,
        }
    }
}

const MANIFESTS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "setup.cfg",
    "go.mod",
    "Gemfile",
    "composer.json",
    "rust-toolchain",
    "rust-toolchain.toml",
    "rustfmt.toml",
    ".rustfmt.toml",
    "clippy.toml",
    "deny.toml",
    ".gitignore",
    ".gitattributes",
    ".dockerignore",
    ".editorconfig",
    ".npmrc",
    ".nvmrc",
    "tsconfig.json",
];

fn classify(path: &str) -> Kind {
    let name = file_name(path);
    let dirs: Vec<&str> = path.split('/').collect();
    let dirs = &dirs[..dirs.len() - 1];

    if path.starts_with(".github/workflows/")
        || path.starts_with(".circleci/")
        || name == ".gitlab-ci.yml"
        || name == ".travis.yml"
        || name == "Jenkinsfile"
    {
        return Kind::Ci;
    }
    if dirs
        .iter()
        .any(|d| matches!(*d, "tests" | "test" | "__tests__" | "spec"))
        || name.contains("_test.")
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.contains("_spec.")
        || (name.starts_with("test_") && name.ends_with(".py"))
    {
        return Kind::Test;
    }
    if is_lockfile(path)
        || MANIFESTS.contains(&name)
        || name.starts_with(".prettierrc")
        || name.starts_with(".eslintrc")
        || (name.starts_with("requirements") && name.ends_with(".txt"))
    {
        return Kind::Chore;
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if matches!(ext, "md" | "rst" | "txt" | "adoc")
        || matches!(dirs.first(), Some(&"docs") | Some(&"doc"))
        || matches!(name, "LICENSE" | "CHANGELOG" | "AUTHORS")
    {
        return Kind::Docs;
    }
    Kind::Code
}

const GROUP_DIRS: &[&str] = &["crates", "packages", "apps", "libs", "modules"];
const SOURCE_DIRS: &[&str] = &["src", "lib", "tests", "test", "docs", "doc"];
const GENERIC_STEMS: &[&str] = &["main", "lib", "mod", "index", "init", "__init__"];

/// Module a single path belongs to, if any.
fn module_of(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 2 {
        return None;
    }
    let raw = if GROUP_DIRS.contains(&parts[0]) && parts.len() > 2 {
        parts[1]
    } else if SOURCE_DIRS.contains(&parts[0]) {
        if parts.len() > 2 {
            parts[1]
        } else {
            let stem = parts[1].split('.').next().unwrap_or("");
            if GENERIC_STEMS.contains(&stem) {
                return None;
            }
            stem
        }
    } else if parts[0].starts_with('.') {
        return None;
    } else {
        parts[0]
    };
    sanitize_scope(raw)
}

/// Lowercase, `[a-z0-9._/-]` only, runs of other chars collapsed to `-`.
pub fn sanitize_scope(raw: &str) -> Option<String> {
    let mut out = String::new();
    for c in raw.trim().chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-') {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out
        .trim_matches(|c| matches!(c, '-' | '.' | '/'))
        .to_string();
    (!out.is_empty()).then_some(out)
}

pub fn infer(files: &[FileChange]) -> Hints {
    let mut hints = Hints::default();
    if files.is_empty() {
        return hints;
    }

    let kinds: Vec<Kind> = files.iter().map(|f| classify(&f.path)).collect();
    if kinds.iter().all(|k| *k == kinds[0]) {
        hints.locked_type = kinds[0].locked_type();
    }
    if hints.locked_type.is_none() && files.iter().all(|f| f.status == Status::Added) {
        hints.suggested_type = Some("feat");
    }

    let mut weights: HashMap<Option<String>, usize> = HashMap::new();
    let mut total = 0;
    for f in files {
        let w = f.changed_lines().max(1);
        total += w;
        *weights.entry(module_of(&f.path)).or_default() += w;
    }
    hints.scope = weights
        .into_iter()
        .filter(|(_, w)| w * 10 >= total * 6)
        .find_map(|(m, _)| m);
    hints
}

#[cfg(test)]
mod tests;
