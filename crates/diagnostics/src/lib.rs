//! Structured logging: one JSON object per line, easy to grep and to ship.
//!
//! Modules log with the `tracing` macros and structured fields, e.g.
//! `tracing::warn!(module = "dwm", hresult, "attribute rejected")`.
//! The binary calls [`init_file`] once at startup.

use std::{
    fs::{self, OpenOptions},
    io,
    path::Path,
    sync::Mutex,
};

use tracing_subscriber::{EnvFilter, fmt::MakeWriter};

/// Default verbosity; override with the `VENEER_LOG` environment variable
/// (standard `EnvFilter` syntax, e.g. `VENEER_LOG=debug`).
const DEFAULT_FILTER: &str = "info";
const FILTER_ENV: &str = "VENEER_LOG";

fn filter() -> EnvFilter {
    EnvFilter::try_from_env(FILTER_ENV).unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER))
}

/// Installs the global JSON logger, appending to `path`.
/// Fails if the file cannot be opened or a logger is already installed.
pub fn init_file(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter())
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .try_init()
        .map_err(io::Error::other)
}

/// Builds a JSON subscriber writing to `writer`, without installing it
/// globally. Used by tests and by tools that want scoped logging.
pub fn json_subscriber<W>(writer: W) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::new(DEFAULT_FILTER))
        .with_writer(writer)
        .with_ansi(false)
        .finish()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Buffer {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Buffer {
        type Writer = Buffer;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn lines(buffer: &Buffer) -> Vec<serde_json::Value> {
        String::from_utf8(buffer.0.lock().unwrap().clone())
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).expect("every line must be valid JSON"))
            .collect()
    }

    #[test]
    fn events_are_one_json_object_per_line_with_structured_fields() {
        let buffer = Buffer::default();
        tracing::subscriber::with_default(json_subscriber(buffer.clone()), || {
            tracing::warn!(
                module = "dwm",
                hresult = -2147024809_i64,
                "attribute rejected"
            );
            tracing::info!("second");
        });
        let out = lines(&buffer);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["level"], "WARN");
        assert_eq!(out[0]["fields"]["message"], "attribute rejected");
        assert_eq!(out[0]["fields"]["module"], "dwm");
        assert_eq!(out[0]["fields"]["hresult"], -2147024809_i64);
        assert!(out[0]["timestamp"].is_string());
    }

    #[test]
    fn debug_events_are_filtered_out_by_default() {
        let buffer = Buffer::default();
        tracing::subscriber::with_default(json_subscriber(buffer.clone()), || {
            tracing::debug!("noise");
        });
        assert!(lines(&buffer).is_empty());
    }

    #[test]
    fn init_file_rejects_unwritable_paths() {
        let dir = tempfile::TempDir::new().unwrap();
        // A directory in place of the log file cannot be opened for writing.
        let blocked = dir.path().join("blocked");
        fs::create_dir(&blocked).unwrap();
        assert!(init_file(&blocked).is_err());
    }
}
