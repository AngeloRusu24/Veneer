//! Orchestration: theme text in, styled window out, one call to undo.
//!
//! [`Controller`] is generic over a [`FrameBackend`], so every decision (version
//! checks, the `unsupported` policy, rule resolution, undo-before-reapply) is
//! tested without touching Windows.

use std::{collections::HashSet, fmt};

pub mod applier;
pub mod wallpaper;

use serde::Serialize;
use veneer_rule_engine::{WindowContext, resolve};
use veneer_theme_schema::{Theme, ThemeError, TimeOfDay, UnsupportedPolicy, parse_theme};
use veneer_windows_platform::{
    backend::FrameBackend,
    plan::{Failure, Skipped, plan_window_frame},
};

/// Why a theme was not applied. Nothing is changed when an error is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    Theme(ThemeError),
    /// The Windows build could not be detected, so nothing can be planned safely.
    UnknownBuild,
    /// The theme asks for a newer Windows build and its policy is `error`.
    BuildTooOld {
        required: u32,
        running: u32,
    },
    /// Some properties are unsupported here and the theme's policy is `error`.
    Unsupported(Vec<String>),
    /// The wallpaper could not be set and the theme's policy is `error`.
    Wallpaper(String),
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wallpaper(reason) => write!(f, "{reason}"),
            Self::Theme(e) => write!(f, "{e}"),
            Self::UnknownBuild => f.write_str("could not detect the Windows version"),
            Self::BuildTooOld { required, running } => write!(
                f,
                "the theme requires Windows build {required}, this is build {running}"
            ),
            Self::Unsupported(names) => write!(
                f,
                "not supported on this Windows version: {}",
                names.join(", ")
            ),
        }
    }
}

impl std::error::Error for ApplyError {}

impl From<ThemeError> for ApplyError {
    fn from(error: ThemeError) -> Self {
        Self::Theme(error)
    }
}

/// Basic facts about a valid theme, for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSummary {
    pub id: String,
    pub name: String,
    pub rule_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedProperty {
    pub property: String,
    pub required_build: u32,
    pub running_build: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedProperty {
    pub property: String,
    pub hresult: i32,
}

/// What happened when a theme was applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub theme_id: String,
    pub matched_rules: Vec<String>,
    pub applied: Vec<String>,
    pub skipped: Vec<SkippedProperty>,
    pub failed: Vec<FailedProperty>,
    pub warnings: Vec<String>,
}

/// What happened when a window was reverted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertReport {
    /// `false` if the window had no active style (nothing to undo).
    pub was_styled: bool,
    pub failed: Vec<FailedProperty>,
}

fn skipped_report(skipped: &[Skipped]) -> Vec<SkippedProperty> {
    skipped
        .iter()
        .map(|s| SkippedProperty {
            property: s.attribute.name().to_owned(),
            required_build: s.required_build,
            running_build: s.running_build,
        })
        .collect()
}

fn failed_report(failures: &[Failure]) -> Vec<FailedProperty> {
    failures
        .iter()
        .map(|f| FailedProperty {
            property: f.attribute.name().to_owned(),
            hresult: f.hresult,
        })
        .collect()
}

/// Validates theme text without applying anything.
pub fn validate_theme(input: &str) -> Result<ThemeSummary, ThemeError> {
    let theme = parse_theme(input)?;
    Ok(summary(&theme))
}

fn summary(theme: &Theme) -> ThemeSummary {
    ThemeSummary {
        id: theme.id.clone(),
        name: theme.name.clone(),
        rule_count: theme.rules.len(),
    }
}

#[derive(Debug)]
pub struct Controller<B> {
    backend: B,
    build: Option<u32>,
    styled: HashSet<isize>,
}

impl<B: FrameBackend> Controller<B> {
    pub fn new(backend: B, build: Option<u32>) -> Self {
        Self {
            backend,
            build,
            styled: HashSet::new(),
        }
    }

    pub fn is_styled(&self, window: isize) -> bool {
        self.styled.contains(&window)
    }

    /// Parses `theme_json`, resolves it for `context` at `now`, and applies the
    /// result to `window`.
    ///
    /// All checks happen before the first write, so an error never leaves the
    /// window half-styled. A previously applied style is undone first, which
    /// keeps the original values intact across repeated applications.
    pub fn apply_theme(
        &mut self,
        window: isize,
        theme_json: &str,
        context: &WindowContext,
        now: TimeOfDay,
    ) -> Result<ApplyReport, ApplyError> {
        let theme = parse_theme(theme_json)?;
        let build = self.build.ok_or(ApplyError::UnknownBuild)?;
        let mut warnings = Vec::new();

        if let Some(required) = theme.target.minimum_build
            && build < required
        {
            match theme.unsupported {
                UnsupportedPolicy::Error => {
                    return Err(ApplyError::BuildTooOld {
                        required,
                        running: build,
                    });
                }
                UnsupportedPolicy::Warn => warnings.push(format!(
                    "the theme is designed for Windows build {required}, this is build {build}"
                )),
            }
        }

        let resolution = resolve(&theme, context, now);
        let plan = plan_window_frame(&resolution.window_frame, build);

        if !plan.skipped.is_empty() {
            let names: Vec<String> = plan
                .skipped
                .iter()
                .map(|s| s.attribute.name().to_owned())
                .collect();
            match theme.unsupported {
                UnsupportedPolicy::Error => return Err(ApplyError::Unsupported(names)),
                UnsupportedPolicy::Warn => warnings.push(format!(
                    "skipped on this Windows version: {}",
                    names.join(", ")
                )),
            }
        }

        if self.styled.remove(&window) {
            let leftovers = self.backend.restore(window);
            if !leftovers.is_empty() {
                tracing::warn!(?leftovers, "previous style not fully restored");
            }
        }

        let outcome = self.backend.apply(window, &plan);
        if !outcome.applied.is_empty() {
            self.styled.insert(window);
        }
        for failure in &outcome.failed {
            tracing::warn!(
                property = failure.attribute.name(),
                hresult = failure.hresult,
                "window property rejected"
            );
        }
        tracing::info!(
            theme = %theme.id,
            rules = resolution.matched_rules.len(),
            applied = outcome.applied.len(),
            "theme applied"
        );

        Ok(ApplyReport {
            theme_id: theme.id,
            matched_rules: resolution.matched_rules,
            applied: outcome
                .applied
                .iter()
                .map(|o| o.attribute.name().to_owned())
                .collect(),
            skipped: skipped_report(&plan.skipped),
            failed: failed_report(&outcome.failed),
            warnings,
        })
    }

    /// Undoes the style on `window`, if any.
    pub fn revert(&mut self, window: isize) -> RevertReport {
        if !self.styled.remove(&window) {
            return RevertReport {
                was_styled: false,
                failed: Vec::new(),
            };
        }
        let failed = self.backend.restore(window);
        tracing::info!(failed = failed.len(), "style reverted");
        RevertReport {
            was_styled: true,
            failed: failed_report(&failed),
        }
    }

    /// Reverts every styled window (used on shutdown).
    pub fn revert_all(&mut self) -> Vec<RevertReport> {
        let windows: Vec<isize> = self.styled.iter().copied().collect();
        windows.into_iter().map(|w| self.revert(w)).collect()
    }
}

#[cfg(test)]
mod tests;
