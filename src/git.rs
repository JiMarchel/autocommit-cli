//! Thin wrapper over the `git` CLI.

use crate::diff::{self, FileChange};
use anyhow::{Context, Result, bail};
use std::process::Command;

fn git(args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .context("failed to run git (is it installed?)")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn ensure_repo() -> Result<()> {
    git(&["rev-parse", "--git-dir"])
        .map(|_| ())
        .context("not inside a git repository")
}

pub fn staged_changes() -> Result<Vec<FileChange>> {
    // Normalize user config that changes the patch header format, so that
    // `diff::parse` can map blocks to paths.
    let base = [
        "-c",
        "core.quotePath=false",
        "diff",
        "--cached",
        "--no-renames",
        "--no-color",
        "--no-ext-diff",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ];
    let with = |extra: &[&str]| {
        let args: Vec<&str> = base.iter().chain(extra).copied().collect();
        git(&args)
    };
    let name_status = with(&["-z", "--name-status"])?;
    let numstat = with(&["-z", "--numstat"])?;
    let patch = with(&["--unified=3"])?;
    Ok(diff::parse(&name_status, &numstat, &patch))
}

/// Commit with `message`; hooks still run. With `edit`, git opens its own
/// configured editor (GIT_EDITOR / core.editor / VISUAL / EDITOR) on the
/// message first, which works the same on Linux, macOS and Windows.
pub fn commit(message: &str, edit: bool) -> Result<()> {
    // `-F -` with `-e` would make git's editor fight us for stdin, so hand the
    // message over in a private temp file instead.
    let dir = tempfile::Builder::new()
        .prefix("acm-")
        .tempdir()
        .context("failed to create a temp dir")?;
    let path = dir.path().join("COMMIT_MSG");
    std::fs::write(&path, format!("{message}\n"))?;

    let mut cmd = Command::new("git");
    cmd.args(["commit", "--cleanup=strip", "-F"]).arg(&path);
    if edit {
        cmd.arg("--edit");
    }
    let status = cmd.status().context("failed to run git commit")?;
    if !status.success() {
        if edit {
            bail!("git commit aborted or failed ({status}); nothing committed");
        }
        bail!("git commit failed ({status})");
    }
    Ok(())
}
