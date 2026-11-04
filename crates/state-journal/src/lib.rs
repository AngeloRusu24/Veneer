//! Write-ahead journal of original system state, for crash-safe rollback.
//!
//! Protocol for any module that changes something outside Veneer's own memory:
//!
//! 1. read the current (original) value,
//! 2. [`Journal::record`] it (persisted to disk before returning),
//! 3. only then change the system,
//! 4. to undo: restore the value, then [`Journal::forget`] it.
//!
//! If Veneer dies between 3 and 4, the journal file still holds the original, so
//! the next start calls [`Journal::recover`] and the system returns to its
//! initial state. A non-empty journal at startup therefore means "unclean exit".
//!
//! The journal knows nothing about Windows: modules store an opaque JSON value
//! and provide a [`Restorer`] that knows how to put it back.

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Owning module, e.g. `"accent"`; lets a restorer dispatch.
    pub module: String,
    /// Unique target within the module, e.g. a registry value or window identity.
    pub key: String,
    /// The value to put back, in a module-defined shape.
    pub original: Value,
}

#[derive(Serialize, Deserialize)]
struct FileFormat {
    version: u32,
    entries: Vec<Entry>,
}

/// What [`Journal::open`] found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// No journal file: the previous run exited cleanly (or this is the first run).
    Clean,
    /// A journal with this many pending entries: the previous run did not exit cleanly.
    Pending(usize),
    /// The file was unreadable or of an unknown version. It was moved aside to
    /// `backup` and an empty journal started, so Veneer can still run.
    Corrupt { backup: PathBuf },
}

/// Why a restore attempt failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError {
    /// The target no longer exists (e.g. a window was closed). Nothing to undo,
    /// so the entry is dropped.
    Gone(String),
    /// A transient failure. The entry is kept so the next start can retry.
    Retry(String),
}

pub trait Restorer {
    fn restore(&mut self, entry: &Entry) -> Result<(), RestoreError>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    pub restored: usize,
    pub gone: usize,
    pub failed: Vec<(Entry, String)>,
}

impl RecoveryReport {
    pub fn is_complete(&self) -> bool {
        self.failed.is_empty()
    }
}

#[derive(Debug)]
pub struct Journal {
    path: PathBuf,
    entries: Vec<Entry>,
}

impl Journal {
    /// Opens (or starts) the journal at `path`. Never fails because of a bad
    /// journal file, only because of real I/O errors such as a read-only disk.
    pub fn open(path: impl Into<PathBuf>) -> io::Result<(Self, Opened)> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok((Self::empty(path), Opened::Clean));
            }
            Err(e) => return Err(e),
        };

        match serde_json::from_slice::<FileFormat>(&bytes) {
            Ok(file) if file.version == FORMAT_VERSION => {
                let pending = file.entries.len();
                let opened = if pending == 0 {
                    Opened::Clean
                } else {
                    Opened::Pending(pending)
                };
                Ok((
                    Self {
                        path,
                        entries: file.entries,
                    },
                    opened,
                ))
            }
            _ => {
                let backup = path.with_extension("corrupt");
                fs::rename(&path, &backup)?;
                tracing::error!(backup = %backup.display(), "journal unreadable, moved aside");
                Ok((Self::empty(path), Opened::Corrupt { backup }))
            }
        }
    }

    fn empty(path: PathBuf) -> Self {
        Self {
            path,
            entries: Vec::new(),
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Records the original value of `(module, key)` and persists it.
    ///
    /// Only the **first** snapshot per target is kept: applying a style twice
    /// must not make the already-modified state look like the original.
    /// Returns `true` if a new snapshot was stored.
    pub fn record(&mut self, module: &str, key: &str, original: Value) -> io::Result<bool> {
        if self.find(module, key).is_some() {
            return Ok(false);
        }
        self.entries.push(Entry {
            module: module.to_owned(),
            key: key.to_owned(),
            original,
        });
        if let Err(e) = self.save() {
            self.entries.pop();
            return Err(e);
        }
        Ok(true)
    }

    /// Drops the snapshot after the original was restored. Returns whether it existed.
    pub fn forget(&mut self, module: &str, key: &str) -> io::Result<bool> {
        let Some(index) = self.find(module, key) else {
            return Ok(false);
        };
        let removed = self.entries.remove(index);
        if let Err(e) = self.save() {
            self.entries.insert(index, removed);
            return Err(e);
        }
        Ok(true)
    }

    /// Restores every pending entry, newest first, and persists what is left.
    ///
    /// One failing entry never stops the others.
    pub fn recover(&mut self, restorer: &mut dyn Restorer) -> io::Result<RecoveryReport> {
        let mut report = RecoveryReport::default();
        let mut kept = Vec::new();

        for entry in self.entries.drain(..).rev() {
            match restorer.restore(&entry) {
                Ok(()) => {
                    tracing::info!(module = %entry.module, key = %entry.key, "restored");
                    report.restored += 1;
                }
                Err(RestoreError::Gone(reason)) => {
                    tracing::warn!(module = %entry.module, key = %entry.key, %reason, "target gone");
                    report.gone += 1;
                }
                Err(RestoreError::Retry(reason)) => {
                    tracing::error!(module = %entry.module, key = %entry.key, %reason, "restore failed");
                    report.failed.push((entry.clone(), reason));
                    kept.push(entry);
                }
            }
        }

        kept.reverse();
        self.entries = kept;
        self.save()?;
        Ok(report)
    }

    fn find(&self, module: &str, key: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.module == module && e.key == key)
    }

    /// Atomic save: write a temp file, flush it to disk, then rename over the
    /// real one, so a crash mid-write can never leave a half-written journal.
    /// An empty journal is deleted, which is what makes "file exists" mean "dirty".
    fn save(&self) -> io::Result<()> {
        if self.entries.is_empty() {
            return match fs::remove_file(&self.path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        let tmp = self.path.with_extension("tmp");
        write_synced(
            &tmp,
            &serde_json::to_vec_pretty(&FileFormat {
                version: FORMAT_VERSION,
                entries: self.entries.clone(),
            })
            .map_err(io::Error::other)?,
        )?;
        fs::rename(&tmp, &self.path)
    }
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests;
