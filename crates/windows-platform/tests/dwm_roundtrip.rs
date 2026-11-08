//! Integration test: applies a style to a real (hidden) top-level window,
//! reads the values back through DWM, then restores and checks the original
//! state. Uses the predefined `STATIC` window class so no class registration
//! or message loop is needed.
#![cfg(windows)]

use veneer_theme_schema::{Backdrop, ColorMode, CornerPreference, Rgb, WindowFrame};
use veneer_windows_platform::{
    backend::FrameBackend,
    dwm::{DwmBackend, apply_plan, read_attribute},
    os::build_number,
    plan::{DwmAttribute, colorref, plan_window_frame},
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
};
use windows::core::w;

struct TestWindow(isize);

impl TestWindow {
    fn new() -> Self {
        // SAFETY: plain window creation with valid constant arguments.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("veneer-test"),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                200,
                200,
                None,
                None,
                None,
                None,
            )
        }
        .expect("could not create a test window");
        Self(hwnd.0 as isize)
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        // SAFETY: the handle was created by this struct and is destroyed once.
        let _ = unsafe { DestroyWindow(windows::Win32::Foundation::HWND(self.0 as *mut _)) };
    }
}

fn frame() -> WindowFrame {
    WindowFrame {
        color_mode: Some(ColorMode::Dark),
        border_color: Some(Rgb {
            r: 0x20,
            g: 0x40,
            b: 0x60,
        }),
        caption_color: Some(Rgb {
            r: 0x10,
            g: 0x10,
            b: 0x18,
        }),
        text_color: Some(Rgb {
            r: 0xEE,
            g: 0xEE,
            b: 0xEE,
        }),
        corner_preference: Some(CornerPreference::RoundSmall),
        backdrop: Some(Backdrop::Mica),
    }
}

#[test]
fn applies_reads_back_and_restores() {
    let build = build_number().expect("build number must be readable");
    assert!(build >= 10240, "unexpected build {build}");

    let window = TestWindow::new();
    let plan = plan_window_frame(&frame(), build);
    assert!(
        !plan.operations.is_empty(),
        "nothing to apply on build {build}"
    );

    let before: Vec<_> = plan
        .operations
        .iter()
        .map(|o| (o.attribute, read_attribute(window.0, o.attribute)))
        .collect();

    let applied = apply_plan(window.0, &plan);
    assert!(applied.failed.is_empty(), "failures: {:?}", applied.failed);
    assert_eq!(applied.applied.len(), plan.operations.len());

    // DWM does not allow reading the color attributes back (E_INVALIDARG), so
    // only the others can be verified; for colors a successful write is the check.
    for op in applied.applied.iter().filter(|o| !is_color(o.attribute)) {
        assert_eq!(
            read_attribute(window.0, op.attribute),
            Ok(op.value),
            "{:?} was not applied",
            op.attribute
        );
    }

    assert!(applied.restore(window.0).is_empty());
    for (attribute, original) in before {
        if let (Ok(original), false) = (original, is_color(attribute)) {
            assert_eq!(
                read_attribute(window.0, attribute),
                Ok(original),
                "{attribute:?} was not restored"
            );
        }
    }
}

#[test]
fn invalid_window_reports_failures_instead_of_panicking() {
    let build = build_number().unwrap();
    let plan = plan_window_frame(&frame(), build);
    let applied = apply_plan(0x1, &plan);
    assert!(applied.applied.is_empty());
    assert_eq!(applied.failed.len(), plan.operations.len());
}

#[test]
fn restore_after_window_is_destroyed_is_a_no_op() {
    let build = build_number().unwrap();
    let plan = plan_window_frame(&frame(), build);
    let mut backend = DwmBackend::default();
    let handle = {
        let window = TestWindow::new();
        let outcome = backend.apply(window.0, &plan);
        assert!(!outcome.applied.is_empty());
        window.0
    };
    assert!(backend.restore(handle).is_empty());
}

fn is_color(attribute: DwmAttribute) -> bool {
    matches!(
        attribute,
        DwmAttribute::BorderColor | DwmAttribute::CaptionColor | DwmAttribute::TextColor
    )
}

#[test]
fn color_write_is_accepted_and_restorable() {
    let build = build_number().unwrap();
    if build < DwmAttribute::BorderColor.min_build() {
        return;
    }
    let window = TestWindow::new();
    let color = Rgb {
        r: 0x12,
        g: 0x34,
        b: 0x56,
    };
    let plan = plan_window_frame(
        &WindowFrame {
            border_color: Some(color),
            ..WindowFrame::default()
        },
        build,
    );
    assert_eq!(plan.operations[0].value, colorref(color));
    let applied = apply_plan(window.0, &plan);
    assert!(applied.failed.is_empty(), "failures: {:?}", applied.failed);
    assert!(applied.restore(window.0).is_empty());
}
