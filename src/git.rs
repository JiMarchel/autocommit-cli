//! Thin wrapper over the `git` CLI.

use crate::diff::{self, FileChange};
use anyhow::{Context, Result, bail};
use std::io::Write;
use std::process::{Command, Stdio};

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

/// Commit with `message` read from stdin; hooks still run.
pub fn commit(message: &str) -> Result<()> {
    let mut child = Command::new("git")
        .args(["commit", "--cleanup=strip", "-F", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to run git commit")?;
    child
        .stdin
        .take()
        .context("no stdin for git commit")?
        .write_all(message.as_bytes())?;
    let status = child.wait()?;
    if !status.success() {
        bail!("git commit failed ({status})");
    }
    Ok(())
}
