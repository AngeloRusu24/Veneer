//! Thin, safe wrappers over the DWM attribute APIs (`dwmapi.dll`).
//!
//! Every write is preceded by a read of the previous value, so a change can
//! always be undone with [`AppliedFrame::restore`]. A failing attribute never
//! stops the others: failures are collected and returned to the caller.

use std::ffi::c_void;
use std::mem::size_of;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DWMWINDOWATTRIBUTE, DwmGetWindowAttribute, DwmSetWindowAttribute,
};

use crate::plan::{COLOR_DEFAULT, DwmAttribute, Operation, Plan};

/// An attribute operation that Windows rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure {
    pub attribute: DwmAttribute,
    /// The `HRESULT` returned by DWM.
    pub hresult: i32,
}

/// The result of applying a [`Plan`] to one window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedFrame {
    pub applied: Vec<Operation>,
    pub failed: Vec<Failure>,
    /// Values to write back on restore, one per applied operation.
    previous: Vec<Operation>,
}

fn hwnd_from_raw(raw: isize) -> HWND {
    HWND(raw as *mut c_void)
}

/// Value to restore when the original could not be read. DWM refuses to read
/// the color attributes back (`E_INVALIDARG`), so for those this is the path
/// that is always taken: "default" hands control back to the system.
fn system_default(attribute: DwmAttribute) -> u32 {
    match attribute {
        DwmAttribute::BorderColor | DwmAttribute::CaptionColor | DwmAttribute::TextColor => {
            COLOR_DEFAULT
        }
        DwmAttribute::UseImmersiveDarkMode
        | DwmAttribute::WindowCornerPreference
        | DwmAttribute::SystemBackdropType => 0,
    }
}

/// Reads a 4-byte DWM attribute. Returns the `HRESULT` on failure.
pub fn read_attribute(hwnd: isize, attribute: DwmAttribute) -> Result<u32, i32> {
    let mut value: u32 = 0;
    // SAFETY: `value` is a valid, writable 4-byte buffer for the whole call and
    // `cbattribute` matches its size. An invalid HWND only yields an error code.
    unsafe {
        DwmGetWindowAttribute(
            hwnd_from_raw(hwnd),
            DWMWINDOWATTRIBUTE(attribute.id()),
            (&mut value as *mut u32).cast::<c_void>(),
            size_of::<u32>() as u32,
        )
    }
    .map(|()| value)
    .map_err(|e| e.code().0)
}

fn write_attribute(hwnd: isize, attribute: DwmAttribute, value: u32) -> Result<(), i32> {
    // SAFETY: `value` is a valid 4-byte buffer readable for the whole call and
    // `cbattribute` matches its size.
    unsafe {
        DwmSetWindowAttribute(
            hwnd_from_raw(hwnd),
            DWMWINDOWATTRIBUTE(attribute.id()),
            (&value as *const u32).cast::<c_void>(),
            size_of::<u32>() as u32,
        )
    }
    .map_err(|e| e.code().0)
}

/// Applies `plan` to the window, remembering what it overwrote.
pub fn apply_plan(hwnd: isize, plan: &Plan) -> AppliedFrame {
    let mut result = AppliedFrame {
        applied: Vec::new(),
        failed: Vec::new(),
        previous: Vec::new(),
    };
    for operation in &plan.operations {
        let before = read_attribute(hwnd, operation.attribute)
            .unwrap_or_else(|_| system_default(operation.attribute));
        match write_attribute(hwnd, operation.attribute, operation.value) {
            Ok(()) => {
                result.applied.push(*operation);
                result.previous.push(Operation {
                    attribute: operation.attribute,
                    value: before,
                });
            }
            Err(hresult) => result.failed.push(Failure {
                attribute: operation.attribute,
                hresult,
            }),
        }
    }
    result
}

impl AppliedFrame {
    /// Writes the original values back, in reverse order. Returns the
    /// attributes that could not be restored (e.g. the window is gone).
    pub fn restore(&self, hwnd: isize) -> Vec<Failure> {
        self.previous
            .iter()
            .rev()
            .filter_map(|op| {
                write_attribute(hwnd, op.attribute, op.value)
                    .err()
                    .map(|hresult| Failure {
                        attribute: op.attribute,
                        hresult,
                    })
            })
            .collect()
    }
}
