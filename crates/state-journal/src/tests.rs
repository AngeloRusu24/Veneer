use std::fs;

use serde_json::json;
use tempfile::TempDir;

use super::*;

fn journal_in(dir: &TempDir) -> (Journal, Opened) {
    Journal::open(dir.path().join("state").join("journal.json")).unwrap()
}

#[derive(Default)]
struct Fake {
    order: Vec<String>,
    gone: Vec<&'static str>,
    retry: Vec<&'static str>,
}

impl Restorer for Fake {
    fn restore(&mut self, entry: &Entry) -> Result<(), RestoreError> {
        if self.gone.contains(&entry.key.as_str()) {
            return Err(RestoreError::Gone("closed".into()));
        }
        if self.retry.contains(&entry.key.as_str()) {
            return Err(RestoreError::Retry("busy".into()));
        }
        self.order.push(entry.key.clone());
        Ok(())
    }
}

#[test]
fn starts_clean_and_creates_parent_directories() {
    let dir = TempDir::new().unwrap();
    let (journal, opened) = journal_in(&dir);
    assert_eq!(opened, Opened::Clean);
    assert!(journal.is_empty());
}

#[test]
fn recorded_state_survives_a_restart() {
    let dir = TempDir::new().unwrap();
    {
        let (mut journal, _) = journal_in(&dir);
        assert!(journal.record("accent", "color", json!("#112233")).unwrap());
    } // dropped without any clean shutdown: simulates a crash

    let (journal, opened) = journal_in(&dir);
    assert_eq!(opened, Opened::Pending(1));
    assert_eq!(journal.entries()[0].original, json!("#112233"));
}

#[test]
fn only_the_first_snapshot_is_kept() {
    let dir = TempDir::new().unwrap();
    let (mut journal, _) = journal_in(&dir);
    assert!(journal.record("m", "k", json!("original")).unwrap());
    assert!(!journal.record("m", "k", json!("already-modified")).unwrap());
    assert_eq!(journal.entries().len(), 1);
    assert_eq!(journal.entries()[0].original, json!("original"));
}

#[test]
fn same_key_in_different_modules_is_independent() {
    let dir = TempDir::new().unwrap();
    let (mut journal, _) = journal_in(&dir);
    assert!(journal.record("a", "k", json!(1)).unwrap());
    assert!(journal.record("b", "k", json!(2)).unwrap());
    assert_eq!(journal.entries().len(), 2);
}

#[test]
fn forgetting_the_last_entry_deletes_the_file() {
    let dir = TempDir::new().unwrap();
    let (mut journal, _) = journal_in(&dir);
    journal.record("m", "k", json!(1)).unwrap();
    assert!(journal.path.exists());
    assert!(journal.forget("m", "k").unwrap());
    assert!(!journal.path.exists());
    assert!(!journal.forget("m", "k").unwrap());

    let (_, opened) = journal_in(&dir);
    assert_eq!(opened, Opened::Clean);
}

#[test]
fn recovery_restores_newest_first_and_empties_the_journal() {
    let dir = TempDir::new().unwrap();
    let (mut journal, _) = journal_in(&dir);
    for key in ["first", "second", "third"] {
        journal.record("m", key, json!(key)).unwrap();
    }
    let mut restorer = Fake::default();
    let report = journal.recover(&mut restorer).unwrap();

    assert_eq!(restorer.order, ["third", "second", "first"]);
    assert_eq!(report.restored, 3);
    assert!(report.is_complete());
    assert!(journal.is_empty());
    assert!(!journal.path.exists());
}

#[test]
fn recovery_drops_gone_targets_and_keeps_retryable_ones() {
    let dir = TempDir::new().unwrap();
    let (mut journal, _) = journal_in(&dir);
    for key in ["ok", "gone", "busy"] {
        journal.record("m", key, json!(key)).unwrap();
    }
    let mut restorer = Fake {
        gone: vec!["gone"],
        retry: vec!["busy"],
        ..Fake::default()
    };
    let report = journal.recover(&mut restorer).unwrap();

    assert_eq!((report.restored, report.gone), (1, 1));
    assert!(!report.is_complete());
    assert_eq!(journal.entries().len(), 1);
    assert_eq!(journal.entries()[0].key, "busy");

    let (reloaded, opened) = journal_in(&dir);
    assert_eq!(opened, Opened::Pending(1));
    assert_eq!(reloaded.entries()[0].key, "busy");
}

#[test]
fn one_failure_does_not_stop_the_others() {
    let dir = TempDir::new().unwrap();
    let (mut journal, _) = journal_in(&dir);
    for key in ["a", "b", "c"] {
        journal.record("m", key, json!(0)).unwrap();
    }
    let mut restorer = Fake {
        retry: vec!["b"],
        ..Fake::default()
    };
    journal.recover(&mut restorer).unwrap();
    assert_eq!(restorer.order, ["c", "a"]);
}

#[test]
fn corrupt_journal_is_moved_aside_and_does_not_block_startup() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("journal.json");
    fs::write(&path, b"{ this is not json").unwrap();

    let (journal, opened) = Journal::open(&path).unwrap();
    let Opened::Corrupt { backup } = opened else {
        panic!("expected Corrupt, got {opened:?}");
    };
    assert!(backup.exists());
    assert!(!path.exists());
    assert!(journal.is_empty());
}

#[test]
fn unknown_format_version_is_treated_as_corrupt() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("journal.json");
    fs::write(&path, br#"{"version":99,"entries":[]}"#).unwrap();
    let (_, opened) = Journal::open(&path).unwrap();
    assert!(matches!(opened, Opened::Corrupt { .. }));
}

#[test]
fn saving_leaves_no_temp_file_behind() {
    let dir = TempDir::new().unwrap();
    let (mut journal, _) = journal_in(&dir);
    journal
        .record("m", "k", json!({"nested": [1, 2, 3]}))
        .unwrap();
    assert!(!journal.path.with_extension("tmp").exists());

    let (reloaded, _) = journal_in(&dir);
    assert_eq!(reloaded.entries()[0].original, json!({"nested": [1, 2, 3]}));
}

#[test]
fn failed_save_does_not_leave_a_phantom_entry() {
    let dir = TempDir::new().unwrap();
    let (mut journal, _) = journal_in(&dir);
    // A directory where the temp file should go makes the save fail.
    fs::create_dir(journal.path.with_extension("tmp")).unwrap();
    assert!(journal.record("m", "k", json!(1)).is_err());
    assert!(journal.is_empty());
}
