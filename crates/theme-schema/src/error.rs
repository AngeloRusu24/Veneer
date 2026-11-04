use std::{error::Error, fmt};

/// A single problem found in a theme, with the JSON path where it occurs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

impl Issue {
    pub(crate) fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeError {
    TooLarge {
        size: usize,
        max: usize,
    },
    Syntax(String),
    MissingSchemaVersion,
    UnsupportedSchemaVersion {
        found: u64,
        supported: u32,
    },
    /// The document does not match the schema structure (wrong type, unknown field, bad value).
    Schema(Issue),
    /// The document is structurally valid but violates semantic rules.
    Invalid(Vec<Issue>),
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { size, max } => {
                write!(f, "theme is {size} bytes, the maximum is {max}")
            }
            Self::Syntax(message) => write!(f, "invalid JSON: {message}"),
            Self::MissingSchemaVersion => write!(f, "missing required field `schemaVersion`"),
            Self::UnsupportedSchemaVersion { found, supported } => write!(
                f,
                "schemaVersion {found} is not supported (this build supports version {supported})"
            ),
            Self::Schema(issue) => write!(f, "{issue}"),
            Self::Invalid(issues) => {
                write!(f, "theme is invalid:")?;
                for issue in issues {
                    write!(f, "\n  - {issue}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for ThemeError {}
