//! Declarative, versioned theme files: parsing and validation.
//!
//! The entry point is [`parse_theme`]. It is pure (no I/O, no Windows APIs) so it
//! can be tested exhaustively and reused by the UI and the agent.

mod error;
mod model;
mod validate;

pub use error::{Issue, ThemeError};
pub use model::*;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;
pub const MAX_THEME_BYTES: usize = 1024 * 1024;

/// Parses and validates a theme document.
///
/// Checks run in this order so that errors are as specific as possible:
/// size, JSON syntax, `schemaVersion`, strict structure, semantic rules.
pub fn parse_theme(input: &str) -> Result<Theme, ThemeError> {
    if input.len() > MAX_THEME_BYTES {
        return Err(ThemeError::TooLarge {
            size: input.len(),
            max: MAX_THEME_BYTES,
        });
    }

    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| ThemeError::Syntax(e.to_string()))?;

    let object = value
        .as_object()
        .ok_or_else(|| ThemeError::Schema(Issue::new("$", "theme must be a JSON object")))?;

    // Checked before the strict parse so a newer theme gets a clear version error
    // instead of a confusing "unknown field" error.
    match object.get("schemaVersion") {
        None => return Err(ThemeError::MissingSchemaVersion),
        Some(version) => match version.as_u64() {
            Some(found) if found == u64::from(CURRENT_SCHEMA_VERSION) => {}
            Some(found) => {
                return Err(ThemeError::UnsupportedSchemaVersion {
                    found,
                    supported: CURRENT_SCHEMA_VERSION,
                });
            }
            None => {
                return Err(ThemeError::Schema(Issue::new(
                    "schemaVersion",
                    "must be a positive integer",
                )));
            }
        },
    }

    let theme: Theme = serde_path_to_error::deserialize(value)
        .map_err(|e| ThemeError::Schema(Issue::new(e.path().to_string(), e.inner().to_string())))?;

    validate::validate(&theme)?;
    Ok(theme)
}

#[cfg(test)]
mod tests;
