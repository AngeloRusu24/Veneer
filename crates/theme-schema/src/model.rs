use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Theme {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub target: Target,
    /// What to do when a requested property cannot be applied.
    #[serde(default)]
    pub unsupported: UnsupportedPolicy,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    /// Minimum Windows build number the theme is designed for.
    pub minimum_build: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnsupportedPolicy {
    #[default]
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    #[serde(default)]
    pub when: Condition,
    pub apply: Style,
}

/// All present conditions must match. An empty condition always matches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Condition {
    pub process_name: Option<String>,
    pub window_title_contains: Option<String>,
    pub monitor: Option<MonitorSelector>,
    pub time_range: Option<TimeRange>,
    pub foreground: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MonitorSelector {
    Primary,
    NonPrimary,
}

/// Half-open interval `[from, to)`. If `from > to` the range wraps past midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeRange {
    pub from: TimeOfDay,
    pub to: TimeOfDay,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Style {
    pub window_frame: Option<WindowFrame>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowFrame {
    pub color_mode: Option<ColorMode>,
    pub border_color: Option<Rgb>,
    pub caption_color: Option<Rgb>,
    pub text_color: Option<Rgb>,
    pub corner_preference: Option<CornerPreference>,
    pub backdrop: Option<Backdrop>,
}

impl WindowFrame {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColorMode {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CornerPreference {
    Default,
    Round,
    RoundSmall,
    DoNotRound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Backdrop {
    Auto,
    None,
    Mica,
    Acrylic,
    Tabbed,
}

/// An opaque color written as `#RRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl FromStr for Rgb {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s
            .strip_prefix('#')
            .filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| format!("color `{s}` must use the #RRGGBB format"))?;
        let value = u32::from_str_radix(hex, 16).map_err(|e| e.to_string())?;
        Ok(Self {
            r: (value >> 16) as u8,
            g: (value >> 8) as u8,
            b: value as u8,
        })
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

/// A local time of day with minute precision, written as `HH:MM` (24-hour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeOfDay {
    minutes: u16,
}

impl TimeOfDay {
    /// Returns `None` if `minutes` is not within a single day (0..1440).
    pub fn from_minutes(minutes: u16) -> Option<Self> {
        (minutes < 24 * 60).then_some(Self { minutes })
    }

    pub fn minutes_since_midnight(self) -> u16 {
        self.minutes
    }
}

impl FromStr for TimeOfDay {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let error = || format!("time `{s}` must use the 24-hour HH:MM format");
        let (hours, minutes) = s.split_once(':').ok_or_else(error)?;
        let is_two_digits =
            |part: &str| part.len() == 2 && part.bytes().all(|b| b.is_ascii_digit());
        if !is_two_digits(hours) || !is_two_digits(minutes) {
            return Err(error());
        }
        let (hours, minutes): (u16, u16) = (
            hours.parse().map_err(|_| error())?,
            minutes.parse().map_err(|_| error())?,
        );
        if hours > 23 || minutes > 59 {
            return Err(error());
        }
        Ok(Self {
            minutes: hours * 60 + minutes,
        })
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.minutes / 60, self.minutes % 60)
    }
}

impl Serialize for TimeOfDay {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for TimeOfDay {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}
