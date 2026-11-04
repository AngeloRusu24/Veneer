use std::collections::HashSet;

use crate::{
    error::{Issue, ThemeError},
    model::{Condition, Theme},
};

const MAX_NAME_CHARS: usize = 100;
const MAX_RULES: usize = 256;
const MAX_SLUG_LEN: usize = 64;
/// First public Windows 10 build; lower values cannot be meaningful.
const MIN_WINDOWS_BUILD: u32 = 10240;

/// Checks the semantic rules that the structural schema cannot express.
/// Collects every problem so authors can fix them in one pass.
pub(crate) fn validate(theme: &Theme) -> Result<(), ThemeError> {
    let mut issues = Vec::new();

    if !is_slug(&theme.id) {
        issues.push(slug_issue("id", &theme.id));
    }

    if theme.name.trim().is_empty() || theme.name.chars().count() > MAX_NAME_CHARS {
        issues.push(Issue::new(
            "name",
            format!("must be between 1 and {MAX_NAME_CHARS} characters"),
        ));
    }

    if let Some(build) = theme.target.minimum_build
        && build < MIN_WINDOWS_BUILD
    {
        issues.push(Issue::new(
            "target.minimumBuild",
            format!("must be at least {MIN_WINDOWS_BUILD} (the first Windows 10 build)"),
        ));
    }

    if theme.rules.len() > MAX_RULES {
        issues.push(Issue::new(
            "rules",
            format!("at most {MAX_RULES} rules are allowed"),
        ));
    }

    let mut seen_ids = HashSet::new();
    for (index, rule) in theme.rules.iter().enumerate() {
        let path = format!("rules[{index}]");

        if !is_slug(&rule.id) {
            issues.push(slug_issue(&format!("{path}.id"), &rule.id));
        } else if !seen_ids.insert(rule.id.as_str()) {
            issues.push(Issue::new(
                format!("{path}.id"),
                format!("duplicate rule id `{}`", rule.id),
            ));
        }

        validate_condition(&rule.when, &format!("{path}.when"), &mut issues);

        match &rule.apply.window_frame {
            Some(frame) if !frame.is_empty() => {}
            Some(_) => issues.push(Issue::new(
                format!("{path}.apply.windowFrame"),
                "must set at least one property",
            )),
            None => issues.push(Issue::new(
                format!("{path}.apply"),
                "must set at least one style property",
            )),
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(ThemeError::Invalid(issues))
    }
}

fn validate_condition(condition: &Condition, path: &str, issues: &mut Vec<Issue>) {
    if let Some(process) = &condition.process_name
        && (process.trim().is_empty() || process.contains(['\\', '/']))
    {
        issues.push(Issue::new(
            format!("{path}.processName"),
            "must be a non-empty executable name without a path",
        ));
    }

    if let Some(title) = &condition.window_title_contains
        && title.trim().is_empty()
    {
        issues.push(Issue::new(
            format!("{path}.windowTitleContains"),
            "must not be empty",
        ));
    }

    if let Some(range) = &condition.time_range
        && range.from == range.to
    {
        issues.push(Issue::new(
            format!("{path}.timeRange"),
            "`from` and `to` must differ",
        ));
    }
}

fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SLUG_LEN
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn slug_issue(path: &str, value: &str) -> Issue {
    Issue::new(
        path,
        format!(
            "`{value}` must be 1-{MAX_SLUG_LEN} characters of lowercase letters, digits and hyphens, \
             without a leading or trailing hyphen"
        ),
    )
}
