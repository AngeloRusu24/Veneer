//! Pure planning of DWM attribute changes.
//!
//! Given the style a theme wants and the Windows build we run on, decide which
//! DWM attributes to set and which to skip (and why). No Windows API is called
//! here, so the logic is testable on any machine and any simulated build.

use veneer_theme_schema::{Backdrop, ColorMode, CornerPreference, Rgb, WindowFrame};

/// First Windows 10 build (2004) where `DWMWA_USE_IMMERSIVE_DARK_MODE = 20` works.
pub const BUILD_DARK_MODE: u32 = 19041;
/// First Windows 11 build: rounded corners, border/caption/text colors.
pub const BUILD_WINDOWS_11: u32 = 22000;
/// Windows 11 22H2: `DWMWA_SYSTEMBACKDROP_TYPE` (Mica, Acrylic, Tabbed).
pub const BUILD_SYSTEM_BACKDROP: u32 = 22621;

/// DWM window attributes Veneer can set. Discriminants are the official
/// `DWMWINDOWATTRIBUTE` values from `dwmapi.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum DwmAttribute {
    UseImmersiveDarkMode = 20,
    WindowCornerPreference = 33,
    BorderColor = 34,
    CaptionColor = 35,
    TextColor = 36,
    SystemBackdropType = 38,
}

impl DwmAttribute {
    pub const fn id(self) -> i32 {
        self as i32
    }

    /// Minimum Windows build on which the attribute is documented to work.
    pub const fn min_build(self) -> u32 {
        match self {
            Self::UseImmersiveDarkMode => BUILD_DARK_MODE,
            Self::WindowCornerPreference
            | Self::BorderColor
            | Self::CaptionColor
            | Self::TextColor => BUILD_WINDOWS_11,
            Self::SystemBackdropType => BUILD_SYSTEM_BACKDROP,
        }
    }
}

/// One attribute write. Every DWM attribute used here is a 4-byte value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operation {
    pub attribute: DwmAttribute,
    pub value: u32,
}

/// A requested property that cannot be applied on this Windows build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Skipped {
    pub attribute: DwmAttribute,
    pub required_build: u32,
    pub running_build: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub operations: Vec<Operation>,
    pub skipped: Vec<Skipped>,
}

/// `COLORREF` layout is `0x00BBGGRR`.
pub fn colorref(color: Rgb) -> u32 {
    u32::from(color.r) | u32::from(color.g) << 8 | u32::from(color.b) << 16
}

/// Value meaning "use the system default" for the color attributes.
pub const COLOR_DEFAULT: u32 = 0xFFFF_FFFF;

fn corner_value(preference: CornerPreference) -> u32 {
    match preference {
        CornerPreference::Default => 0,
        CornerPreference::DoNotRound => 1,
        CornerPreference::Round => 2,
        CornerPreference::RoundSmall => 3,
    }
}

fn backdrop_value(backdrop: Backdrop) -> u32 {
    match backdrop {
        Backdrop::Auto => 0,
        Backdrop::None => 1,
        Backdrop::Mica => 2,
        Backdrop::Acrylic => 3,
        Backdrop::Tabbed => 4,
    }
}

/// Builds the list of attribute writes for `frame` on Windows build `build`.
pub fn plan_window_frame(frame: &WindowFrame, build: u32) -> Plan {
    let wanted = [
        frame.color_mode.map(|m| Operation {
            attribute: DwmAttribute::UseImmersiveDarkMode,
            value: u32::from(m == ColorMode::Dark),
        }),
        frame.corner_preference.map(|c| Operation {
            attribute: DwmAttribute::WindowCornerPreference,
            value: corner_value(c),
        }),
        frame.border_color.map(|c| Operation {
            attribute: DwmAttribute::BorderColor,
            value: colorref(c),
        }),
        frame.caption_color.map(|c| Operation {
            attribute: DwmAttribute::CaptionColor,
            value: colorref(c),
        }),
        frame.text_color.map(|c| Operation {
            attribute: DwmAttribute::TextColor,
            value: colorref(c),
        }),
        frame.backdrop.map(|b| Operation {
            attribute: DwmAttribute::SystemBackdropType,
            value: backdrop_value(b),
        }),
    ];

    let mut plan = Plan::default();
    for operation in wanted.into_iter().flatten() {
        let required_build = operation.attribute.min_build();
        if build >= required_build {
            plan.operations.push(operation);
        } else {
            plan.skipped.push(Skipped {
                attribute: operation.attribute,
                required_build,
                running_build: build,
            });
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_frame() -> WindowFrame {
        WindowFrame {
            color_mode: Some(ColorMode::Dark),
            border_color: Some(Rgb {
                r: 0x11,
                g: 0x22,
                b: 0x33,
            }),
            caption_color: Some(Rgb {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
            }),
            text_color: Some(Rgb {
                r: 0xFF,
                g: 0xFF,
                b: 0xFF,
            }),
            corner_preference: Some(CornerPreference::Round),
            backdrop: Some(Backdrop::Mica),
        }
    }

    fn attributes(plan: &Plan) -> Vec<DwmAttribute> {
        plan.operations.iter().map(|o| o.attribute).collect()
    }

    #[test]
    fn colorref_is_bgr_ordered() {
        assert_eq!(
            colorref(Rgb {
                r: 0x11,
                g: 0x22,
                b: 0x33
            }),
            0x0033_2211
        );
    }

    #[test]
    fn empty_frame_plans_nothing() {
        let plan = plan_window_frame(&WindowFrame::default(), 26100);
        assert_eq!(plan, Plan::default());
    }

    #[test]
    fn everything_applies_on_recent_windows_11() {
        let plan = plan_window_frame(&full_frame(), 26100);
        assert_eq!(plan.operations.len(), 6);
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn windows_10_only_gets_dark_mode() {
        let plan = plan_window_frame(&full_frame(), 19045);
        assert_eq!(attributes(&plan), [DwmAttribute::UseImmersiveDarkMode]);
        assert_eq!(plan.skipped.len(), 5);
        assert!(plan.skipped.iter().all(|s| s.running_build == 19045));
    }

    #[test]
    fn windows_11_21h2_has_colors_but_no_backdrop() {
        let plan = plan_window_frame(&full_frame(), 22000);
        assert_eq!(plan.operations.len(), 5);
        assert_eq!(
            plan.skipped.iter().map(|s| s.attribute).collect::<Vec<_>>(),
            [DwmAttribute::SystemBackdropType]
        );
    }

    #[test]
    fn very_old_build_skips_everything() {
        let plan = plan_window_frame(&full_frame(), 18362);
        assert!(plan.operations.is_empty());
        assert_eq!(plan.skipped.len(), 6);
    }

    #[test]
    fn build_boundaries_are_inclusive() {
        let dark = WindowFrame {
            color_mode: Some(ColorMode::Dark),
            ..WindowFrame::default()
        };
        assert!(
            plan_window_frame(&dark, BUILD_DARK_MODE - 1)
                .operations
                .is_empty()
        );
        assert_eq!(
            plan_window_frame(&dark, BUILD_DARK_MODE).operations.len(),
            1
        );
    }

    #[test]
    fn enum_values_match_the_official_headers() {
        let frame = WindowFrame {
            color_mode: Some(ColorMode::Light),
            corner_preference: Some(CornerPreference::RoundSmall),
            backdrop: Some(Backdrop::Tabbed),
            ..WindowFrame::default()
        };
        let plan = plan_window_frame(&frame, 26100);
        let value = |a: DwmAttribute| {
            plan.operations
                .iter()
                .find(|o| o.attribute == a)
                .unwrap()
                .value
        };
        assert_eq!(value(DwmAttribute::UseImmersiveDarkMode), 0);
        assert_eq!(value(DwmAttribute::WindowCornerPreference), 3);
        assert_eq!(value(DwmAttribute::SystemBackdropType), 4);
    }

    #[test]
    fn attribute_ids_match_dwmapi_h() {
        assert_eq!(DwmAttribute::UseImmersiveDarkMode.id(), 20);
        assert_eq!(DwmAttribute::WindowCornerPreference.id(), 33);
        assert_eq!(DwmAttribute::BorderColor.id(), 34);
        assert_eq!(DwmAttribute::CaptionColor.id(), 35);
        assert_eq!(DwmAttribute::TextColor.id(), 36);
        assert_eq!(DwmAttribute::SystemBackdropType.id(), 38);
    }
}
