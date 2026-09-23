use super::*;
use crate::diff::{FileChange, Status};

fn f(path: &str, lines: usize) -> FileChange {
    FileChange::new(path, Status::Modified, lines, 0)
}

fn added(path: &str) -> FileChange {
    FileChange::new(path, Status::Added, 5, 0)
}

#[test]
fn docs_only_locks_docs() {
    let h = infer(&[f("README.md", 3), f("docs/guide/setup.rst", 4)]);
    assert_eq!(h.locked_type, Some("docs"));
}

#[test]
fn tests_only_locks_test() {
    let h = infer(&[
        f("tests/cli.rs", 3),
        f("src/foo_test.go", 1),
        f("web/a.spec.ts", 1),
        f("test_x.py", 1),
    ]);
    assert_eq!(h.locked_type, Some("test"));
}

#[test]
fn ci_only_locks_ci() {
    let h = infer(&[f(".github/workflows/ci.yml", 3), f(".gitlab-ci.yml", 1)]);
    assert_eq!(h.locked_type, Some("ci"));
}

#[test]
fn manifests_only_lock_chore() {
    let h = infer(&[f("Cargo.toml", 3), f("Cargo.lock", 30), f(".gitignore", 1)]);
    assert_eq!(h.locked_type, Some("chore"));
}

#[test]
fn mixed_changes_are_not_locked() {
    let h = infer(&[f("src/main.rs", 3), f("README.md", 1)]);
    assert_eq!(h.locked_type, None);
    assert_eq!(h.suggested_type, None);
}

#[test]
fn all_new_code_files_suggest_feat() {
    let h = infer(&[added("src/git.rs"), added("src/app.rs")]);
    assert_eq!(h.locked_type, None);
    assert_eq!(h.suggested_type, Some("feat"));
}

#[test]
fn scope_from_src_subdir_and_file_stem() {
    assert_eq!(
        infer(&[f("src/diff/tests.rs", 5)]).scope.as_deref(),
        Some("diff")
    );
    assert_eq!(
        infer(&[f("src/gemini.rs", 5)]).scope.as_deref(),
        Some("gemini")
    );
    assert_eq!(infer(&[f("web/app.ts", 5)]).scope.as_deref(), Some("web"));
    assert_eq!(
        infer(&[f("crates/core/src/lib.rs", 5)]).scope.as_deref(),
        Some("core")
    );
}

#[test]
fn scope_requires_sixty_percent_of_lines() {
    let h = infer(&[f("src/diff.rs", 70), f("src/hints.rs", 30)]);
    assert_eq!(h.scope.as_deref(), Some("diff"));
    let h = infer(&[f("src/diff.rs", 50), f("src/hints.rs", 50)]);
    assert_eq!(h.scope, None);
}

#[test]
fn top_level_files_and_generic_stems_have_no_scope() {
    assert_eq!(infer(&[f("Cargo.toml", 5)]).scope, None);
    assert_eq!(infer(&[f("src/main.rs", 5)]).scope, None);
    assert_eq!(infer(&[f("src/lib.rs", 5)]).scope, None);
}

#[test]
fn scope_is_sanitized() {
    assert_eq!(
        infer(&[f("My App/x.rs", 5)]).scope.as_deref(),
        Some("my-app")
    );
}
