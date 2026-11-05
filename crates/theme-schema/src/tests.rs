use super::*;

fn theme_with_rules(rules: &str) -> String {
    format!(r#"{{"schemaVersion":1,"id":"t","name":"T","rules":{rules}}}"#)
}

fn rule(when: &str, apply: &str) -> String {
    format!(r#"[{{"id":"r1","when":{when},"apply":{apply}}}]"#)
}

fn invalid_issues(input: &str) -> Vec<Issue> {
    match parse_theme(input) {
        Err(ThemeError::Invalid(issues)) => issues,
        other => panic!("expected semantic issues, got {other:?}"),
    }
}

fn schema_issue(input: &str) -> Issue {
    match parse_theme(input) {
        Err(ThemeError::Schema(issue)) => issue,
        other => panic!("expected a schema error, got {other:?}"),
    }
}

#[test]
fn bundled_example_theme_is_valid() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../themes/examples/midnight-work.json"
    );
    let theme = parse_theme(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(theme.id, "midnight-work");
    assert_eq!(theme.rules.len(), 2);
}

#[test]
fn minimal_theme_uses_defaults() {
    let theme = parse_theme(r#"{"schemaVersion":1,"id":"empty","name":"Empty"}"#).unwrap();
    assert!(theme.rules.is_empty());
    assert_eq!(theme.unsupported, UnsupportedPolicy::Warn);
    assert_eq!(theme.target.minimum_build, None);
}

#[test]
fn valid_theme_round_trips_through_serialization() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../themes/examples/midnight-work.json"
    ))
    .unwrap();
    let theme = parse_theme(&source).unwrap();
    let reparsed = parse_theme(&serde_json::to_string(&theme).unwrap()).unwrap();
    assert_eq!(theme, reparsed);
}

#[test]
fn rejects_invalid_json() {
    assert!(matches!(parse_theme("{"), Err(ThemeError::Syntax(_))));
}

#[test]
fn rejects_non_object_document() {
    assert_eq!(schema_issue("[]").path, "$");
}

#[test]
fn rejects_oversized_input() {
    let input = " ".repeat(MAX_THEME_BYTES + 1);
    assert!(matches!(
        parse_theme(&input),
        Err(ThemeError::TooLarge { .. })
    ));
}

#[test]
fn requires_schema_version() {
    assert_eq!(
        parse_theme(r#"{"id":"a","name":"A"}"#),
        Err(ThemeError::MissingSchemaVersion)
    );
}

#[test]
fn reports_future_versions_clearly_even_with_unknown_fields() {
    let result = parse_theme(r#"{"schemaVersion":2,"id":"a","name":"A","newField":true}"#);
    assert_eq!(
        result,
        Err(ThemeError::UnsupportedSchemaVersion {
            found: 2,
            supported: CURRENT_SCHEMA_VERSION
        })
    );
}

#[test]
fn rejects_non_integer_schema_version() {
    assert_eq!(
        schema_issue(r#"{"schemaVersion":"1","id":"a","name":"A"}"#).path,
        "schemaVersion"
    );
}

#[test]
fn rejects_unknown_fields_with_their_location() {
    let issue = schema_issue(&theme_with_rules(&rule(
        "{}",
        r#"{"windowFrame":{"colorMode":"dark","glow":true}}"#,
    )));
    assert_eq!(issue.path, "rules[0].apply.windowFrame.glow");
}

#[test]
fn rejects_unknown_enum_values_with_their_location() {
    let issue = schema_issue(&theme_with_rules(&rule(
        "{}",
        r#"{"windowFrame":{"backdrop":"glass"}}"#,
    )));
    assert_eq!(issue.path, "rules[0].apply.windowFrame.backdrop");
}

#[test]
fn rejects_malformed_colors() {
    for bad in ["red", "#FFF", "#GG0000", "123456", "#1234567", "#ÀÀÀ"] {
        let apply = format!(r#"{{"windowFrame":{{"borderColor":"{bad}"}}}}"#);
        let issue = schema_issue(&theme_with_rules(&rule("{}", &apply)));
        assert_eq!(
            issue.path, "rules[0].apply.windowFrame.borderColor",
            "{bad}"
        );
    }
}

#[test]
fn color_parsing_is_case_insensitive_and_normalized() {
    let color: Rgb = "#aBc012".parse().unwrap();
    assert_eq!(
        color,
        Rgb {
            r: 0xAB,
            g: 0xC0,
            b: 0x12
        }
    );
    assert_eq!(color.to_string(), "#ABC012");
}

#[test]
fn rejects_malformed_times() {
    for bad in ["24:00", "7:00", "07:60", "0700", "07:0", "ab:cd", ""] {
        let when = format!(r#"{{"timeRange":{{"from":"{bad}","to":"08:00"}}}}"#);
        let issue = schema_issue(&theme_with_rules(&rule(
            &when,
            r#"{"windowFrame":{"colorMode":"dark"}}"#,
        )));
        assert_eq!(issue.path, "rules[0].when.timeRange.from", "{bad}");
    }
}

#[test]
fn accepts_time_ranges_that_wrap_midnight() {
    let theme = parse_theme(&theme_with_rules(&rule(
        r#"{"timeRange":{"from":"22:30","to":"06:15"}}"#,
        r#"{"windowFrame":{"colorMode":"dark"}}"#,
    )))
    .unwrap();
    let range = theme.rules[0].when.time_range.unwrap();
    assert_eq!(range.from.minutes_since_midnight(), 22 * 60 + 30);
    assert_eq!(range.to.minutes_since_midnight(), 6 * 60 + 15);
}

#[test]
fn rejects_empty_time_range() {
    let issues = invalid_issues(&theme_with_rules(&rule(
        r#"{"timeRange":{"from":"08:00","to":"08:00"}}"#,
        r#"{"windowFrame":{"colorMode":"dark"}}"#,
    )));
    assert_eq!(issues[0].path, "rules[0].when.timeRange");
}

#[test]
fn rejects_invalid_ids() {
    for bad in [
        "",
        "Upper",
        "has space",
        "-lead",
        "trail-",
        "a_b",
        &"x".repeat(65),
    ] {
        let input = format!(r#"{{"schemaVersion":1,"id":"{bad}","name":"T"}}"#);
        assert_eq!(invalid_issues(&input)[0].path, "id", "{bad:?}");
    }
}

#[test]
fn rejects_duplicate_rule_ids() {
    let apply = r#"{"windowFrame":{"colorMode":"dark"}}"#;
    let input = theme_with_rules(&format!(
        r#"[{{"id":"same","apply":{apply}}},{{"id":"same","apply":{apply}}}]"#
    ));
    let issues = invalid_issues(&input);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].path, "rules[1].id");
}

#[test]
fn rejects_rules_that_change_nothing() {
    let issues = invalid_issues(&theme_with_rules(&rule("{}", "{}")));
    assert_eq!(issues[0].path, "rules[0].apply");

    let issues = invalid_issues(&theme_with_rules(&rule("{}", r#"{"windowFrame":{}}"#)));
    assert_eq!(issues[0].path, "rules[0].apply.windowFrame");
}

#[test]
fn rejects_bad_process_and_title_conditions() {
    let issues = invalid_issues(&theme_with_rules(&rule(
        r#"{"processName":"C:\\apps\\a.exe","windowTitleContains":"  "}"#,
        r#"{"windowFrame":{"colorMode":"dark"}}"#,
    )));
    let paths: Vec<_> = issues.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "rules[0].when.processName",
            "rules[0].when.windowTitleContains"
        ]
    );
}

#[test]
fn rejects_implausible_minimum_build() {
    let issues =
        invalid_issues(r#"{"schemaVersion":1,"id":"a","name":"A","target":{"minimumBuild":7601}}"#);
    assert_eq!(issues[0].path, "target.minimumBuild");
}

#[test]
fn reports_all_semantic_problems_at_once() {
    let issues = invalid_issues(
        r#"{"schemaVersion":1,"id":"Bad Id","name":"  ","rules":[{"id":"r","apply":{}}]}"#,
    );
    let paths: Vec<_> = issues.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, ["id", "name", "rules[0].apply"]);
}

#[test]
fn error_messages_are_human_readable() {
    let error = parse_theme(r#"{"schemaVersion":1,"id":"Bad","name":"A"}"#).unwrap_err();
    let text = error.to_string();
    assert!(text.starts_with("theme is invalid:"), "{text}");
    assert!(text.contains("id:"), "{text}");
}

fn theme_with_wallpaper(path: &str) -> String {
    format!(
        r#"{{"schemaVersion":1,"id":"t","name":"T","wallpaper":{{"path":{}}}}}"#,
        serde_json::to_string(path).unwrap()
    )
}

#[test]
fn wallpaper_accepts_absolute_static_images() {
    for path in [
        r"C:\Images\wall.jpg",
        r"d:/pics/Wall.PNG",
        r"\\server\share\a.bmp",
    ] {
        let theme = parse_theme(&theme_with_wallpaper(path)).unwrap();
        assert_eq!(theme.wallpaper.unwrap().path, path);
    }
}

#[test]
fn wallpaper_rejects_relative_paths_and_non_images() {
    for path in [
        "wall.jpg",
        r"..\wall.jpg",
        "",
        r"C:\Images\anim.gif",
        r"C:\Images\clip.mp4",
        r"C:\Images\noextension",
    ] {
        let issues = invalid_issues(&theme_with_wallpaper(path));
        assert_eq!(issues.len(), 1, "{path}: {issues:?}");
        assert_eq!(issues[0].path, "wallpaper.path");
    }
}

#[test]
fn wallpaper_is_optional_and_strict() {
    assert!(
        parse_theme(&theme_with_rules("[]"))
            .unwrap()
            .wallpaper
            .is_none()
    );
    let extra =
        r#"{"schemaVersion":1,"id":"t","name":"T","wallpaper":{"path":"C:\\a.jpg","fit":"fill"}}"#;
    assert!(matches!(parse_theme(extra), Err(ThemeError::Schema(_))));
}

fn theme_with(extra: &str) -> String {
    format!(r#"{{"schemaVersion":1,"id":"t","name":"T",{extra}}}"#)
}

#[test]
fn appearance_accepts_modes_and_rejects_empty_or_unknown() {
    let theme = parse_theme(&theme_with(
        r#""appearance":{"apps":"dark","system":"light"}"#,
    ))
    .unwrap();
    let appearance = theme.appearance.unwrap();
    assert_eq!(appearance.apps, Some(ColorMode::Dark));
    assert_eq!(appearance.system, Some(ColorMode::Light));

    let issues = invalid_issues(&theme_with(r#""appearance":{}"#));
    assert_eq!(issues[0].path, "appearance");
    assert!(matches!(
        parse_theme(&theme_with(r#""appearance":{"apps":"blue"}"#)),
        Err(ThemeError::Schema(_))
    ));
}

#[test]
fn cursors_accept_absolute_cur_and_ani_files() {
    let theme = parse_theme(&theme_with(
        r#""cursors":{"arrow":"C:\\c\\a.cur","wait":"C:\\c\\w.ANI"}"#,
    ))
    .unwrap();
    let entries = theme.cursors.unwrap().entries().len();
    assert_eq!(entries, 2);
}

#[test]
fn cursors_reject_bad_paths_and_empty_sets() {
    let issues = invalid_issues(&theme_with(
        r#""cursors":{"arrow":"a.cur","hand":"C:\\h.png"}"#,
    ));
    let paths: Vec<_> = issues.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, ["cursors.arrow", "cursors.hand"]);

    let issues = invalid_issues(&theme_with(r#""cursors":{}"#));
    assert_eq!(issues[0].path, "cursors");
}
