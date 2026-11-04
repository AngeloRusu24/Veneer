use veneer_theme_schema::{
    Backdrop, ColorMode, CornerPreference, Rgb, Theme, TimeOfDay, parse_theme,
};

use super::*;

fn theme(rules_json: &str) -> Theme {
    parse_theme(&format!(
        r#"{{"schemaVersion":1,"id":"t","name":"T","rules":[{rules_json}]}}"#
    ))
    .expect("test theme must be valid")
}

fn at(hhmm: &str) -> TimeOfDay {
    hhmm.parse().unwrap()
}

fn window(process: &str, title: &str) -> WindowContext {
    WindowContext {
        process_name: process.into(),
        title: title.into(),
        on_primary_monitor: Some(true),
        is_foreground: false,
    }
}

const NOON: &str = "12:00";

#[test]
fn empty_theme_matches_nothing() {
    let r = resolve(&theme(""), &window("a.exe", "x"), at(NOON));
    assert!(r.is_empty());
    assert!(r.window_frame.is_empty());
}

#[test]
fn rule_without_conditions_matches_everything() {
    let t = theme(r#"{"id":"all","apply":{"windowFrame":{"colorMode":"dark"}}}"#);
    let r = resolve(&t, &window("a.exe", "x"), at(NOON));
    assert_eq!(r.matched_rules, ["all"]);
    assert_eq!(r.window_frame.color_mode, Some(ColorMode::Dark));
}

#[test]
fn process_name_is_case_insensitive_and_exact() {
    let t = theme(
        r#"{"id":"code","when":{"processName":"code.exe"},"apply":{"windowFrame":{"colorMode":"dark"}}}"#,
    );
    assert!(!resolve(&t, &window("Code.exe", ""), at(NOON)).is_empty());
    assert!(resolve(&t, &window("Code.exe.bak", ""), at(NOON)).is_empty());
    assert!(resolve(&t, &window("xcode.exe", ""), at(NOON)).is_empty());
}

#[test]
fn title_match_is_case_insensitive_substring() {
    let t = theme(
        r#"{"id":"t","when":{"windowTitleContains":"Report"},"apply":{"windowFrame":{"colorMode":"light"}}}"#,
    );
    assert!(!resolve(&t, &window("a.exe", "Q3 REPORT - Word"), at(NOON)).is_empty());
    assert!(resolve(&t, &window("a.exe", "Notes"), at(NOON)).is_empty());
}

#[test]
fn title_match_handles_non_ascii() {
    let t = theme(
        r#"{"id":"t","when":{"windowTitleContains":"è"},"apply":{"windowFrame":{"colorMode":"light"}}}"#,
    );
    assert!(!resolve(&t, &window("a.exe", "CAFFÈ"), at(NOON)).is_empty());
}

#[test]
fn monitor_selector_and_unknown_monitor() {
    let t = theme(
        r#"{"id":"m","when":{"monitor":"nonPrimary"},"apply":{"windowFrame":{"colorMode":"dark"}}}"#,
    );
    let mut w = window("a.exe", "");
    w.on_primary_monitor = Some(false);
    assert!(!resolve(&t, &w, at(NOON)).is_empty());
    w.on_primary_monitor = Some(true);
    assert!(resolve(&t, &w, at(NOON)).is_empty());
    w.on_primary_monitor = None;
    assert!(resolve(&t, &w, at(NOON)).is_empty());
}

#[test]
fn foreground_condition_both_ways() {
    let t = theme(
        r#"{"id":"f","when":{"foreground":true},"apply":{"windowFrame":{"colorMode":"dark"}}},
           {"id":"b","when":{"foreground":false},"apply":{"windowFrame":{"colorMode":"light"}}}"#,
    );
    let mut w = window("a.exe", "");
    assert_eq!(resolve(&t, &w, at(NOON)).matched_rules, ["b"]);
    w.is_foreground = true;
    assert_eq!(resolve(&t, &w, at(NOON)).matched_rules, ["f"]);
}

#[test]
fn time_range_is_half_open() {
    let t = theme(
        r#"{"id":"day","when":{"timeRange":{"from":"08:00","to":"18:00"}},"apply":{"windowFrame":{"colorMode":"light"}}}"#,
    );
    let w = window("a.exe", "");
    assert!(resolve(&t, &w, at("07:59")).is_empty());
    assert!(!resolve(&t, &w, at("08:00")).is_empty());
    assert!(!resolve(&t, &w, at("17:59")).is_empty());
    assert!(resolve(&t, &w, at("18:00")).is_empty());
}

#[test]
fn time_range_wraps_past_midnight() {
    let t = theme(
        r#"{"id":"night","when":{"timeRange":{"from":"20:00","to":"07:00"}},"apply":{"windowFrame":{"colorMode":"dark"}}}"#,
    );
    let w = window("a.exe", "");
    for hit in ["20:00", "23:59", "00:00", "06:59"] {
        assert!(!resolve(&t, &w, at(hit)).is_empty(), "{hit} should match");
    }
    for miss in ["07:00", "12:00", "19:59"] {
        assert!(
            resolve(&t, &w, at(miss)).is_empty(),
            "{miss} should not match"
        );
    }
}

#[test]
fn all_conditions_must_hold() {
    let t = theme(
        r#"{"id":"x","when":{"processName":"a.exe","windowTitleContains":"doc","foreground":true},
            "apply":{"windowFrame":{"colorMode":"dark"}}}"#,
    );
    let mut w = window("a.exe", "My Doc");
    w.is_foreground = true;
    assert!(!resolve(&t, &w, at(NOON)).is_empty());
    w.title = "Other".into();
    assert!(resolve(&t, &w, at(NOON)).is_empty());
}

#[test]
fn later_rule_wins_per_property() {
    let t = theme(
        r##"{"id":"base","apply":{"windowFrame":{"colorMode":"dark","borderColor":"#111111"}}},
           {"id":"over","apply":{"windowFrame":{"borderColor":"#FF0000"}}}"##,
    );
    let r = resolve(&t, &window("a.exe", ""), at(NOON));
    assert_eq!(r.matched_rules, ["base", "over"]);
    assert_eq!(
        r.window_frame.border_color,
        Some("#FF0000".parse().unwrap())
    );
    assert_eq!(r.window_frame.color_mode, Some(ColorMode::Dark));
}

#[test]
fn non_overlapping_properties_combine() {
    let t = theme(
        r##"{"id":"a","apply":{"windowFrame":{"cornerPreference":"round"}}},
           {"id":"b","apply":{"windowFrame":{"backdrop":"mica","textColor":"#FFFFFF"}}}"##,
    );
    let f = resolve(&t, &window("a.exe", ""), at(NOON)).window_frame;
    assert_eq!(f.corner_preference, Some(CornerPreference::Round));
    assert_eq!(f.backdrop, Some(Backdrop::Mica));
    assert_eq!(
        f.text_color,
        Some(Rgb {
            r: 255,
            g: 255,
            b: 255
        })
    );
}

#[test]
fn unmatched_rule_does_not_override() {
    let t = theme(
        r#"{"id":"base","apply":{"windowFrame":{"colorMode":"dark"}}},
           {"id":"never","when":{"processName":"other.exe"},"apply":{"windowFrame":{"colorMode":"light"}}}"#,
    );
    let r = resolve(&t, &window("a.exe", ""), at(NOON));
    assert_eq!(r.matched_rules, ["base"]);
    assert_eq!(r.window_frame.color_mode, Some(ColorMode::Dark));
}

#[test]
fn resolution_is_deterministic() {
    let t = theme(
        r#"{"id":"a","apply":{"windowFrame":{"colorMode":"dark"}}},
           {"id":"b","when":{"foreground":false},"apply":{"windowFrame":{"backdrop":"mica"}}}"#,
    );
    let w = window("a.exe", "t");
    assert_eq!(resolve(&t, &w, at(NOON)), resolve(&t, &w, at(NOON)));
}

#[test]
fn works_with_the_example_theme() {
    let json = include_str!("../../../themes/examples/midnight-work.json");
    let t = parse_theme(json).unwrap();
    // Must not panic and must be stable for any window/time.
    for time in ["00:00", "09:30", "23:59"] {
        let r = resolve(&t, &window("Code.exe", "main.rs"), at(time));
        assert_eq!(r, resolve(&t, &window("Code.exe", "main.rs"), at(time)));
    }
}

#[test]
fn time_of_day_from_minutes_bounds() {
    assert!(TimeOfDay::from_minutes(1439).is_some());
    assert!(TimeOfDay::from_minutes(1440).is_none());
}
