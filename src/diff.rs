//! Staged-diff model: parsing git output, filtering noise, rendering for the prompt.
//!
//! Everything here is pure (no I/O) so it can be unit-tested.

use std::collections::HashMap;
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Added,
    Modified,
    Deleted,
    Unmerged,
    Other,
}

impl Status {
    fn from_code(code: &str) -> Self {
        match code.chars().next() {
            Some('A') => Status::Added,
            Some('M') => Status::Modified,
            Some('D') => Status::Deleted,
            Some('U') => Status::Unmerged,
            _ => Status::Other,
        }
    }

    fn letter(self) -> char {
        match self {
            Status::Added => 'A',
            Status::Modified => 'M',
            Status::Deleted => 'D',
            Status::Unmerged => 'U',
            Status::Other => 'T',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub status: Status,
    pub added: usize,
    pub removed: usize,
    pub binary: bool,
    pub patch: String,
}

impl FileChange {
    pub fn new(path: &str, status: Status, added: usize, removed: usize) -> Self {
        FileChange {
            path: path.to_string(),
            status,
            added,
            removed,
            binary: false,
            patch: String::new(),
        }
    }

    pub fn changed_lines(&self) -> usize {
        self.added + self.removed
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Max patch lines kept per file (including the `diff --git` header).
    pub per_file_lines: usize,
    /// Max total characters of patch text across all files.
    pub total_chars: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            per_file_lines: 150,
            total_chars: 12_000,
        }
    }
}

/// Max number of files listed in the summary.
const MAX_LISTED_FILES: usize = 50;

/// Parse the outputs of
/// `git diff --cached -z --no-renames --name-status`,
/// `git diff --cached -z --no-renames --numstat` and
/// `git diff --cached --no-renames` (the patch).
pub fn parse(name_status: &str, numstat: &str, patch: &str) -> Vec<FileChange> {
    let mut files = Vec::new();
    let mut fields = name_status.split('\0').filter(|s| !s.is_empty());
    while let (Some(code), Some(path)) = (fields.next(), fields.next()) {
        files.push(FileChange::new(path, Status::from_code(code), 0, 0));
    }

    let mut counts: HashMap<&str, (Option<usize>, Option<usize>)> = HashMap::new();
    for entry in numstat.split('\0').filter(|s| !s.is_empty()) {
        let mut parts = entry.splitn(3, '\t');
        if let (Some(a), Some(r), Some(path)) = (parts.next(), parts.next(), parts.next()) {
            counts.insert(path, (a.parse().ok(), r.parse().ok()));
        }
    }
    for f in &mut files {
        match counts.get(f.path.as_str()) {
            Some((Some(a), Some(r))) => {
                f.added = *a;
                f.removed = *r;
            }
            Some(_) => f.binary = true,
            None => {}
        }
    }

    let blocks = group_by_path(split_patch(patch));
    if blocks.len() == files.len() {
        // Same pathspec + --no-renames => git emits files in the same order.
        for (f, (_, block)) in files.iter_mut().zip(blocks) {
            f.patch = block;
        }
    } else {
        let mut by_path: HashMap<String, String> = blocks
            .into_iter()
            .filter_map(|(p, b)| Some((p?, b)))
            .collect();
        for f in &mut files {
            if let Some(block) = by_path.remove(&f.path) {
                f.patch = block;
            }
        }
    }
    for f in &mut files {
        if f.patch.contains("\nBinary files ") || f.patch.starts_with("Binary files ") {
            f.binary = true;
        }
    }
    files
}

/// Split a full patch into per-file blocks, dropping noisy `index` lines.
fn split_patch(patch: &str) -> Vec<String> {
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            blocks.push(vec![line]);
        } else if let Some(block) = blocks.last_mut()
            && !line.starts_with("index ")
        {
            block.push(line);
        }
    }
    blocks.into_iter().map(|b| b.join("\n")).collect()
}

/// Path from `diff --git a/P b/P` (no renames => both sides are equal).
/// Requires the default `a/`/`b/` prefixes and unquoted paths, which
/// `git::staged_changes` forces; returns None for anything else.
fn header_path(block: &str) -> Option<String> {
    let rest = block.lines().next()?.strip_prefix("diff --git ")?;
    let n = rest.len();
    if n < 5 || (n - 5) % 2 != 0 {
        return None;
    }
    let len = (n - 5) / 2;
    let (a, b) = (rest.get(2..2 + len)?, rest.get(len + 5..)?);
    (rest.starts_with("a/") && rest.get(2 + len..len + 5)? == " b/" && a == b)
        .then(|| a.to_string())
}

/// Merge consecutive blocks for the same path (a typechange such as
/// file -> symlink is emitted as a deletion block plus an addition block).
fn group_by_path(blocks: Vec<String>) -> Vec<(Option<String>, String)> {
    let mut out: Vec<(Option<String>, String)> = Vec::new();
    for block in blocks {
        let path = header_path(&block);
        match out.last_mut() {
            Some((Some(prev), text)) if path.as_ref() == Some(prev) => {
                text.push('\n');
                text.push_str(&block);
            }
            _ => out.push((path, block)),
        }
    }
    out
}

const LOCKFILES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lockb",
    "poetry.lock",
    "uv.lock",
    "Pipfile.lock",
    "go.sum",
    "composer.lock",
    "Gemfile.lock",
    "flake.lock",
];

const GENERATED_DIRS: &[&str] = &["dist", "build", "target", "node_modules", "vendor"];

pub fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn is_lockfile(path: &str) -> bool {
    LOCKFILES.contains(&file_name(path))
}

/// Why a file's patch should NOT be sent to the model (it is still listed).
pub fn skip_reason(file: &FileChange) -> Option<&'static str> {
    let name = file_name(&file.path);
    if is_lockfile(&file.path) {
        Some("lockfile")
    } else if file.binary {
        Some("binary")
    } else if name.contains(".min.")
        || name.ends_with(".map")
        || name.ends_with(".snap")
        || file
            .path
            .split('/')
            .rev()
            .skip(1)
            .any(|dir| GENERATED_DIRS.contains(&dir))
    {
        Some("generated")
    } else {
        None
    }
}

fn truncate_patch(patch: &str, max_lines: usize) -> String {
    let total = patch.lines().count();
    if total <= max_lines {
        return patch.to_string();
    }
    let mut out: String = patch.lines().take(max_lines).collect::<Vec<_>>().join("\n");
    let _ = write!(out, "\n... ({} more lines truncated)", total - max_lines);
    out
}

/// Render a compact, bounded description of the change for the prompt:
/// a file summary (every file, capped) followed by the useful patches.
pub fn render(files: &[FileChange], limits: Limits) -> String {
    let mut used = 0;
    let mut patches = Vec::new();
    let mut notes: Vec<Option<&'static str>> = Vec::with_capacity(files.len());

    for f in files {
        if let Some(reason) = skip_reason(f) {
            notes.push(Some(reason));
            continue;
        }
        if f.patch.is_empty() {
            notes.push(None);
            continue;
        }
        let chunk = truncate_patch(&f.patch, limits.per_file_lines);
        if used + chunk.len() > limits.total_chars {
            notes.push(Some("size limit"));
            continue;
        }
        used += chunk.len();
        patches.push(chunk);
        notes.push(None);
    }

    let mut out = String::from("Changed files:\n");
    for (f, note) in files.iter().zip(&notes).take(MAX_LISTED_FILES) {
        let _ = write!(out, "{} {}", f.status.letter(), f.path);
        if f.binary {
            out.push_str(" (binary)");
        } else {
            let _ = write!(out, " (+{} -{})", f.added, f.removed);
        }
        if let Some(reason) = note {
            let _ = write!(out, " [diff omitted: {reason}]");
        }
        out.push('\n');
    }
    if files.len() > MAX_LISTED_FILES {
        let _ = writeln!(out, "... and {} more files", files.len() - MAX_LISTED_FILES);
    }
    if !patches.is_empty() {
        out.push_str("\nDiff:\n");
        out.push_str(&patches.join("\n"));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests;
