//! Deterministic rule evaluation: "which style applies to this window, now?".
//!
//! The engine is a pure function of `(theme, window snapshot, time of day)`.
//! It performs no I/O and never reads the clock, so every decision is
//! reproducible in tests.
//!
//! Resolution works like a CSS cascade: every rule whose conditions match is
//! applied in file order, and for each property the last matching rule wins.
//! Rules that match but set different properties therefore combine.

use veneer_theme_schema::{Condition, Theme, TimeOfDay, TimeRange, WindowFrame};

/// What the engine knows about a window at evaluation time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowContext {
    /// Executable file name, e.g. `Code.exe`.
    pub process_name: String,
    pub title: String,
    /// `None` when the monitor could not be determined; monitor conditions then
    /// do not match.
    pub on_primary_monitor: Option<bool>,
    pub is_foreground: bool,
}

/// The outcome of evaluating a theme for one window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolution {
    pub window_frame: WindowFrame,
    /// Ids of the matching rules, in application order (useful for diagnostics
    /// and for the UI's "why does this window look like this?" view).
    pub matched_rules: Vec<String>,
}

impl Resolution {
    pub fn is_empty(&self) -> bool {
        self.matched_rules.is_empty()
    }
}

/// Evaluates `theme` for `window` at local time `now`.
pub fn resolve(theme: &Theme, window: &WindowContext, now: TimeOfDay) -> Resolution {
    let mut resolution = Resolution::default();
    for rule in &theme.rules {
        if !matches(&rule.when, window, now) {
            continue;
        }
        resolution.matched_rules.push(rule.id.clone());
        if let Some(frame) = &rule.apply.window_frame {
            merge(&mut resolution.window_frame, frame);
        }
    }
    resolution
}

/// Returns whether every present condition holds. An empty condition matches.
pub fn matches(condition: &Condition, window: &WindowContext, now: TimeOfDay) -> bool {
    if let Some(name) = &condition.process_name
        && !window.process_name.eq_ignore_ascii_case(name)
    {
        return false;
    }
    if let Some(needle) = &condition.window_title_contains
        && !window.title.to_lowercase().contains(&needle.to_lowercase())
    {
        return false;
    }
    if let Some(selector) = condition.monitor {
        use veneer_theme_schema::MonitorSelector::{NonPrimary, Primary};
        match (selector, window.on_primary_monitor) {
            (Primary, Some(true)) | (NonPrimary, Some(false)) => {}
            _ => return false,
        }
    }
    if let Some(range) = &condition.time_range
        && !in_time_range(range, now)
    {
        return false;
    }
    if let Some(foreground) = condition.foreground
        && window.is_foreground != foreground
    {
        return false;
    }
    true
}

/// `[from, to)`, wrapping past midnight when `from > to`.
pub fn in_time_range(range: &TimeRange, now: TimeOfDay) -> bool {
    let (from, to, now) = (
        range.from.minutes_since_midnight(),
        range.to.minutes_since_midnight(),
        now.minutes_since_midnight(),
    );
    if from <= to {
        (from..to).contains(&now)
    } else {
        now >= from || now < to
    }
}

fn merge(base: &mut WindowFrame, over: &WindowFrame) {
    base.color_mode = over.color_mode.or(base.color_mode);
    base.border_color = over.border_color.or(base.border_color);
    base.caption_color = over.caption_color.or(base.caption_color);
    base.text_color = over.text_color.or(base.text_color);
    base.corner_preference = over.corner_preference.or(base.corner_preference);
    base.backdrop = over.backdrop.or(base.backdrop);
}

#[cfg(test)]
mod tests;
