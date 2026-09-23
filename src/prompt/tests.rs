use super::*;
use crate::hints::Hints;
use crate::message::Violation;

#[test]
fn build_includes_rules_diff_and_locked_type() {
    let h = Hints {
        locked_type: Some("docs"),
        suggested_type: None,
        scope: Some("guide".into()),
    };
    let p = build(&h, "Changed files:\nM README.md (+1 -0)\n");
    assert!(p.contains("feat, fix, docs"));
    assert!(p.contains("72"));
    assert!(p.contains("MUST be \"docs\""));
    assert!(p.contains("Scope: use \"guide\""));
    assert!(p.contains("M README.md (+1 -0)"));
}

#[test]
fn build_mentions_suggested_type_and_optional_scope() {
    let h = Hints {
        locked_type: None,
        suggested_type: Some("feat"),
        scope: None,
    };
    let p = build(&h, "x");
    assert!(p.contains("probably \"feat\""));
    assert!(p.contains("Scope: optional"));
    assert!(!p.contains("MUST be"));
}

#[test]
fn correction_quotes_previous_answer_and_problem() {
    let p = correction("BASE", "Feat: Add x.", &Violation::TrailingPeriod);
    assert!(p.starts_with("BASE"));
    assert!(p.contains("Feat: Add x."));
    assert!(p.contains(&Violation::TrailingPeriod.to_string()));
}
