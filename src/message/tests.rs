use super::*;
use crate::hints::Hints;

fn hints(
    locked: Option<&'static str>,
    suggested: Option<&'static str>,
    scope: Option<&str>,
) -> Hints {
    Hints {
        locked_type: locked,
        suggested_type: suggested,
        scope: scope.map(str::to_string),
    }
}

// ---- sanitize ----

#[test]
fn sanitize_strips_fences_labels_and_quotes() {
    assert_eq!(sanitize("```\nfeat: add x\n```"), "feat: add x");
    assert_eq!(sanitize("```text\nfeat: add x\n```\n"), "feat: add x");
    assert_eq!(sanitize("Commit message: feat: add x"), "feat: add x");
    assert_eq!(
        sanitize("Here is the commit message:\n\nfeat: add x"),
        "feat: add x"
    );
    assert_eq!(sanitize("\"feat: add x\""), "feat: add x");
    assert_eq!(sanitize("`feat: add x`"), "feat: add x");
    assert_eq!(sanitize("**feat(api): add x**"), "feat(api): add x");
    assert_eq!(sanitize("## feat: add x"), "feat: add x");
}

#[test]
fn sanitize_normalizes_body_spacing() {
    let raw = "  feat: add x  \n- one\n\n\n- two   \n\n";
    assert_eq!(sanitize(raw), "feat: add x\n\n- one\n\n- two");
    assert_eq!(sanitize("feat: add x\n\n\n\n- one"), "feat: add x\n\n- one");
}

#[test]
fn sanitize_keeps_inline_backticks() {
    assert_eq!(
        sanitize("fix: handle `None` input"),
        "fix: handle `None` input"
    );
}

// ---- validate ----

#[test]
fn validate_accepts_good_messages() {
    assert_eq!(validate("feat: add x", None), Ok(()));
    assert_eq!(validate("fix(api)!: drop v1 endpoint", None), Ok(()));
    assert_eq!(validate("docs: update README badges", Some("docs")), Ok(()));
    assert_eq!(
        validate("feat(cli): add flag\n\n- a\n- b\n- c", None),
        Ok(())
    );
}

#[test]
fn validate_reports_violations() {
    assert_eq!(validate("", None), Err(Violation::Empty));
    assert_eq!(validate("add x", None), Err(Violation::BadHeader));
    assert_eq!(validate("feat:add x", None), Err(Violation::BadHeader));
    assert_eq!(validate("feat(): add x", None), Err(Violation::BadHeader));
    assert_eq!(
        validate("feature: add x", None),
        Err(Violation::UnknownType("feature".into()))
    );
    assert_eq!(
        validate("feat: add x", Some("docs")),
        Err(Violation::WrongType {
            expected: "docs",
            found: "feat".into()
        })
    );
    let long = format!("feat: {}", "a".repeat(80));
    assert_eq!(validate(&long, None), Err(Violation::SubjectTooLong(86)));
    assert_eq!(
        validate("feat: add x.", None),
        Err(Violation::TrailingPeriod)
    );
    assert_eq!(
        validate("feat: Add x", None),
        Err(Violation::UppercaseStart)
    );
    assert_eq!(
        validate("feat: add x\n\nsome paragraph", None),
        Err(Violation::BadBody)
    );
    assert_eq!(
        validate("feat: add x\n\n- a\n- b\n- c\n- d", None),
        Err(Violation::BadBody)
    );
}

#[test]
fn violations_have_readable_messages() {
    assert!(Violation::SubjectTooLong(90).to_string().contains("72"));
    assert!(
        Violation::WrongType {
            expected: "docs",
            found: "feat".into()
        }
        .to_string()
        .contains("\"docs\"")
    );
}

// ---- enforce_type ----

#[test]
fn enforce_type_rewrites_type_only() {
    assert_eq!(
        enforce_type("feat(readme): add badges", "docs"),
        "docs(readme): add badges"
    );
    assert_eq!(enforce_type("fix: x\n\n- y", "test"), "test: x\n\n- y");
    assert_eq!(enforce_type("no header here", "docs"), "no header here");
}

// ---- repair ----

#[test]
fn repair_fixes_common_mistakes() {
    let h = hints(None, None, None);
    assert_eq!(repair("feat: Add parser.", &h), "feat: add parser");
    assert_eq!(
        repair("Feature(API): Add parser", &h),
        "feat(api): add parser"
    );
    assert_eq!(
        repair("Added a new parser", &h),
        "chore: added a new parser"
    );
    assert_eq!(repair("feat: README update", &h), "feat: README update");
}

#[test]
fn repair_uses_hints() {
    assert_eq!(
        repair(
            "update the readme",
            &hints(Some("docs"), None, Some("guide"))
        ),
        "docs(guide): update the readme"
    );
    assert_eq!(
        repair("add git module", &hints(None, Some("feat"), None)),
        "feat: add git module"
    );
    assert_eq!(
        repair("fix: typo", &hints(Some("docs"), None, None)),
        "docs: typo"
    );
}

#[test]
fn repair_truncates_at_word_boundary() {
    let raw = format!("feat(scope): {}", "word ".repeat(30));
    let out = repair(&raw, &hints(None, None, None));
    assert!(out.chars().count() <= 72, "{out}");
    assert!(out.starts_with("feat(scope): word word"));
    assert!(out.ends_with("word"));
}

#[test]
fn repair_normalizes_body() {
    let raw = "feat: add x\n\nSome paragraph.\n* one\n• two\n- three\n- four";
    assert_eq!(
        repair(raw, &hints(None, None, None)),
        "feat: add x\n\n- one\n- two\n- three"
    );
}

#[test]
fn repair_never_returns_empty() {
    let out = repair("", &hints(None, None, Some("core")));
    assert_eq!(out, "chore(core): update core");
    assert_eq!(
        repair("...", &hints(None, None, None)),
        "chore: update files"
    );
}

#[test]
fn repair_never_leaves_empty_description_after_truncation() {
    let raw = format!("feat: {}fix", ".".repeat(100));
    assert_eq!(repair(&raw, &hints(None, None, None)), "feat: update files");
    let raw = format!("feat: {} word", "!".repeat(100));
    assert_eq!(
        repair(&raw, &hints(None, None, Some("core"))),
        "feat: update core"
    );
}

#[test]
fn repair_output_always_validates() {
    let inputs = [
        "",
        "Feature: Something BIG happened!!!.",
        "```\nblah\n```",
        "fix(weird scope with spaces): x",
        "feat:",
        "revert: \"feat: add x\"",
        &"x".repeat(200),
        "feat: add x\n\nparagraph one\n\nparagraph two",
    ];
    for locked in [None, Some("docs")] {
        let h = hints(locked, None, Some("core"));
        for raw in inputs {
            let out = repair(&sanitize(raw), &h);
            assert_eq!(validate(&out, locked), Ok(()), "input {raw:?} -> {out:?}");
        }
    }
}
