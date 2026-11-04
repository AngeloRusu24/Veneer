use veneer_windows_platform::{
    backend::ApplyOutcome,
    plan::{DwmAttribute, Plan},
};

use super::*;

#[derive(Default, Debug)]
struct Fake {
    log: Vec<String>,
    reject: Vec<DwmAttribute>,
}

impl FrameBackend for Fake {
    fn apply(&mut self, window: isize, plan: &Plan) -> ApplyOutcome {
        self.log
            .push(format!("apply {window} x{}", plan.operations.len()));
        let mut outcome = ApplyOutcome::default();
        for op in &plan.operations {
            if self.reject.contains(&op.attribute) {
                outcome.failed.push(Failure {
                    attribute: op.attribute,
                    hresult: -1,
                });
            } else {
                outcome.applied.push(*op);
            }
        }
        outcome
    }

    fn restore(&mut self, window: isize) -> Vec<Failure> {
        self.log.push(format!("restore {window}"));
        Vec::new()
    }
}

const WIN11: u32 = 26100;
const WIN10: u32 = 19045;
const W: isize = 7;

fn controller(build: u32) -> Controller<Fake> {
    Controller::new(Fake::default(), Some(build))
}

fn context() -> WindowContext {
    WindowContext {
        process_name: "veneer-desktop.exe".into(),
        title: "Veneer".into(),
        on_primary_monitor: Some(true),
        is_foreground: true,
    }
}

fn noon() -> TimeOfDay {
    "12:00".parse().unwrap()
}

fn theme(extra: &str, rules: &str) -> String {
    format!(r#"{{"schemaVersion":1,"id":"t","name":"T"{extra},"rules":[{rules}]}}"#)
}

const DARK_MICA: &str =
    r#"{"id":"r","apply":{"windowFrame":{"colorMode":"dark","backdrop":"mica"}}}"#;

#[test]
fn applies_a_valid_theme_and_reports_it() {
    let mut c = controller(WIN11);
    let report = c
        .apply_theme(W, &theme("", DARK_MICA), &context(), noon())
        .unwrap();
    assert_eq!(report.theme_id, "t");
    assert_eq!(report.matched_rules, ["r"]);
    assert_eq!(report.applied, ["colorMode", "backdrop"]);
    assert!(report.skipped.is_empty() && report.failed.is_empty() && report.warnings.is_empty());
    assert!(c.is_styled(W));
}

#[test]
fn invalid_theme_changes_nothing() {
    let mut c = controller(WIN11);
    let err = c.apply_theme(W, "{ nope", &context(), noon()).unwrap_err();
    assert!(matches!(err, ApplyError::Theme(_)));
    assert!(c.backend.log.is_empty());
    assert!(!c.is_styled(W));
}

#[test]
fn unknown_build_is_refused() {
    let mut c = Controller::new(Fake::default(), None);
    let err = c
        .apply_theme(W, &theme("", DARK_MICA), &context(), noon())
        .unwrap_err();
    assert_eq!(err, ApplyError::UnknownBuild);
    assert!(c.backend.log.is_empty());
}

#[test]
fn windows_10_warns_and_applies_what_it_can() {
    let mut c = controller(WIN10);
    let report = c
        .apply_theme(W, &theme("", DARK_MICA), &context(), noon())
        .unwrap();
    assert_eq!(report.applied, ["colorMode"]);
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].property, "backdrop");
    assert_eq!(report.skipped[0].required_build, 22621);
    assert_eq!(report.warnings.len(), 1);
}

#[test]
fn error_policy_refuses_unsupported_properties_before_touching_anything() {
    let mut c = controller(WIN10);
    let json = theme(r#","unsupported":"error""#, DARK_MICA);
    let err = c.apply_theme(W, &json, &context(), noon()).unwrap_err();
    assert_eq!(err, ApplyError::Unsupported(vec!["backdrop".into()]));
    assert!(c.backend.log.is_empty());
    assert!(!c.is_styled(W));
}

#[test]
fn minimum_build_follows_the_policy() {
    let target = r#","target":{"minimumBuild":22621}"#;

    let mut c = controller(WIN10);
    let report = c
        .apply_theme(W, &theme(target, DARK_MICA), &context(), noon())
        .unwrap();
    assert!(report.warnings.iter().any(|w| w.contains("22621")));

    let strict = theme(&format!(r#"{target},"unsupported":"error""#), DARK_MICA);
    let err = controller(WIN10)
        .apply_theme(W, &strict, &context(), noon())
        .unwrap_err();
    assert_eq!(
        err,
        ApplyError::BuildTooOld {
            required: 22621,
            running: WIN10
        }
    );
}

#[test]
fn reapplying_undoes_the_previous_style_first() {
    let mut c = controller(WIN11);
    c.apply_theme(W, &theme("", DARK_MICA), &context(), noon())
        .unwrap();
    c.apply_theme(W, &theme("", DARK_MICA), &context(), noon())
        .unwrap();
    assert_eq!(c.backend.log, ["apply 7 x2", "restore 7", "apply 7 x2"]);
}

#[test]
fn revert_restores_once_and_is_idempotent() {
    let mut c = controller(WIN11);
    c.apply_theme(W, &theme("", DARK_MICA), &context(), noon())
        .unwrap();

    let first = c.revert(W);
    assert!(first.was_styled && first.failed.is_empty());
    assert!(!c.is_styled(W));

    let second = c.revert(W);
    assert!(!second.was_styled);
    assert_eq!(c.backend.log, ["apply 7 x2", "restore 7"]);
}

#[test]
fn a_theme_that_matches_nothing_clears_the_old_style() {
    let mut c = controller(WIN11);
    c.apply_theme(W, &theme("", DARK_MICA), &context(), noon())
        .unwrap();
    let other = theme(
        "",
        r#"{"id":"x","when":{"processName":"other.exe"},"apply":{"windowFrame":{"colorMode":"dark"}}}"#,
    );
    let report = c.apply_theme(W, &other, &context(), noon()).unwrap();
    assert!(report.matched_rules.is_empty() && report.applied.is_empty());
    assert!(!c.is_styled(W));
}

#[test]
fn rejected_properties_are_reported_without_failing_the_rest() {
    let mut c = controller(WIN11);
    c.backend.reject.push(DwmAttribute::SystemBackdropType);
    let report = c
        .apply_theme(W, &theme("", DARK_MICA), &context(), noon())
        .unwrap();
    assert_eq!(report.applied, ["colorMode"]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].property, "backdrop");
}

#[test]
fn windows_are_tracked_independently_and_revert_all_covers_them() {
    let mut c = controller(WIN11);
    c.apply_theme(1, &theme("", DARK_MICA), &context(), noon())
        .unwrap();
    c.apply_theme(2, &theme("", DARK_MICA), &context(), noon())
        .unwrap();
    assert_eq!(c.revert_all().len(), 2);
    assert!(!c.is_styled(1) && !c.is_styled(2));
}

#[test]
fn validate_theme_summarises_without_applying() {
    let s = validate_theme(&theme("", DARK_MICA)).unwrap();
    assert_eq!(
        (s.id.as_str(), s.name.as_str(), s.rule_count),
        ("t", "T", 1)
    );
    assert!(validate_theme("[]").is_err());
}

#[test]
fn reports_serialize_with_camel_case_for_the_frontend() {
    let mut c = controller(WIN11);
    let report = c
        .apply_theme(W, &theme("", DARK_MICA), &context(), noon())
        .unwrap();
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains(r#""themeId":"t""#) && json.contains(r#""matchedRules""#));
}
