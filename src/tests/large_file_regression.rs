use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use j3text::app::EditorApp;
use j3text::domain::{
    FileSnapshot, LARGE_FILE_THRESHOLD_BYTES, LineEnding, LoadedDocument, ReadOnlyReason,
    TextEncoding, should_warn_large_file,
};
use j3text::error::AppError;
use j3text::infra::{FileDocumentIo, MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES};

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(prefix: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{nonce}"));
        fs::create_dir_all(&path).expect("create temp root");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn large_file_threshold_policy_is_metadata_driven() {
    assert!(!should_warn_large_file(LARGE_FILE_THRESHOLD_BYTES - 1));
    assert!(should_warn_large_file(LARGE_FILE_THRESHOLD_BYTES));
}

#[test]
fn save_as_recalculates_read_only_policy_from_saved_snapshot_size() {
    assert_save_as_policy(
        ReadOnlyReason::FileAttribute,
        LARGE_FILE_THRESHOLD_BYTES - 1,
        None,
    );
    assert_save_as_policy(
        ReadOnlyReason::LargeFile,
        LARGE_FILE_THRESHOLD_BYTES,
        Some(ReadOnlyReason::LargeFile),
    );
    assert_save_as_policy(
        ReadOnlyReason::LargeFile,
        LARGE_FILE_THRESHOLD_BYTES + 1,
        Some(ReadOnlyReason::LargeFile),
    );
}

#[test]
fn multi_tab_search_state_is_cleared_without_touching_other_documents() {
    let mut app = EditorApp::new();
    app.open_document(j3text::domain::LoadedDocument {
        path: PathBuf::from("first.txt"),
        content: "alpha NEEDLE\nbeta".to_string(),
        encoding: TextEncoding::Utf8,
        line_ending: LineEnding::Lf,
        snapshot: None,
        read_only_reason: None,
    });
    app.open_document(j3text::domain::LoadedDocument {
        path: PathBuf::from("second.txt"),
        content: "second NEEDLE\r\nsecond NEEDLE".to_string(),
        encoding: TextEncoding::Utf8,
        line_ending: LineEnding::Crlf,
        snapshot: None,
        read_only_reason: None,
    });

    app.update_search_results("NEEDLE")
        .expect("search active tab");
    assert_eq!(app.search_results().len(), 2);
    app.set_current_index(0).expect("switch tab");

    assert!(app.search_results().is_empty());
    assert_eq!(
        app.current_document().map(|document| document.content()),
        Some("alpha NEEDLE\nbeta")
    );
}

#[test]
#[ignore = "generates and scans a tens-of-MB file for local performance measurement"]
fn measure_tens_of_mb_load_search_save() {
    let root = TempRoot::new("j3text-large-measure");
    let path = root.path().join("mixed-large.txt");
    write_large_text_file(&path, 48 * 1024 * 1024);

    let io = FileDocumentIo::new();
    let started = Instant::now();
    let loaded = io.load(&path, None, None).expect("load large file");
    eprintln!(
        "load: {:?}, bytes={}, chars={}",
        started.elapsed(),
        loaded.snapshot.expect("snapshot").byte_len,
        loaded.content.chars().count()
    );

    let mut app = EditorApp::new();
    app.open_document(loaded);

    let started = Instant::now();
    app.update_search_results("NEEDLE")
        .expect("search large file");
    eprintln!(
        "search: {:?}, results={}",
        started.elapsed(),
        app.search_results().len()
    );
    assert_eq!(
        app.search_results().len(),
        j3text::domain::MAX_SEARCH_RESULTS
    );

    let save_path = root.path().join("saved-large.txt");
    let document = app.current_document().expect("current document");
    let started = Instant::now();
    io.save(
        &save_path,
        document.content(),
        TextEncoding::Utf8,
        LineEnding::Lf,
        None,
    )
    .expect("save large file");
    eprintln!(
        "save: {:?}, saved_bytes={}",
        started.elapsed(),
        fs::metadata(&save_path).expect("saved metadata").len()
    );
}

#[test]
fn line_ending_normalized_detection_matches_save_policy() {
    assert!(LineEnding::Lf.is_normalized_text("alpha\nbeta"));
    assert!(!LineEnding::Lf.is_normalized_text("alpha\r\nbeta"));
    assert!(!LineEnding::Lf.is_normalized_text("alpha\rbeta"));

    assert!(LineEnding::Cr.is_normalized_text("alpha\rbeta"));
    assert!(!LineEnding::Cr.is_normalized_text("alpha\r\nbeta"));
    assert!(!LineEnding::Cr.is_normalized_text("alpha\nbeta"));

    assert!(LineEnding::Crlf.is_normalized_text("alpha\r\nbeta"));
    assert!(!LineEnding::Crlf.is_normalized_text("alpha\nbeta"));
    assert!(!LineEnding::Crlf.is_normalized_text("alpha\rbeta"));
    assert!(!LineEnding::Crlf.is_normalized_text("alpha\r\r\nbeta"));
}

#[test]
fn large_policy_loads_warn_threshold_read_only() {
    let root = TempRoot::new("j3text-100mb-measure");
    let path = root.path().join("large-policy.txt");
    write_ascii_file(&path, LARGE_FILE_THRESHOLD_BYTES);

    let io = FileDocumentIo::new();
    let loaded = io
        .load(&path, None, Some(ReadOnlyReason::LargeFile))
        .expect("load forced large file");

    assert_eq!(loaded.read_only_reason, Some(ReadOnlyReason::LargeFile));
    assert!(
        loaded
            .snapshot
            .is_some_and(|snapshot| snapshot.byte_len >= LARGE_FILE_THRESHOLD_BYTES)
    );

    let metadata_snapshot = io
        .file_metadata_snapshot(&path)
        .expect("read large file metadata snapshot");
    assert!(metadata_snapshot.byte_len() >= LARGE_FILE_THRESHOLD_BYTES);
}

#[test]
fn confirmed_large_policy_loads_over_threshold_read_only() {
    let root = TempRoot::new("j3text-large-policy-confirmed-over-threshold");
    let path = root.path().join("over-threshold.txt");
    let confirmed_len = LARGE_FILE_THRESHOLD_BYTES + 1;
    write_ascii_file(&path, confirmed_len);

    let loaded = FileDocumentIo::new()
        .load(&path, None, Some(ReadOnlyReason::LargeFile))
        .expect("load confirmed large file read-only");

    assert_eq!(loaded.read_only_reason, Some(ReadOnlyReason::LargeFile));
    assert!(
        loaded
            .snapshot
            .is_some_and(|snapshot| snapshot.byte_len == confirmed_len)
    );
}

#[test]
fn confirmed_large_policy_rejects_above_safe_read_limit() {
    let root = TempRoot::new("j3text-large-policy-above-safe-limit");
    let path = root.path().join("above-safe-limit.txt");
    let oversized_len = MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES + 1;
    let file = File::create(&path).expect("create oversized large-policy file");
    file.set_len(oversized_len)
        .expect("extend oversized large-policy file");

    let error = match FileDocumentIo::new().load(&path, None, Some(ReadOnlyReason::LargeFile)) {
        Ok(_) => panic!("oversized confirmed large file should fail"),
        Err(error) => error,
    };

    match &error {
        AppError::FileTooLarge {
            byte_len, limit, ..
        } => {
            assert_eq!(*byte_len, oversized_len);
            assert_eq!(*limit, MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES);
        }
        other => panic!("expected file-too-large error, got {other:?}"),
    }
}

fn assert_save_as_policy(
    source_reason: ReadOnlyReason,
    saved_byte_len: u64,
    expected_reason: Option<ReadOnlyReason>,
) {
    let mut app = EditorApp::new();
    app.open_document(LoadedDocument {
        path: PathBuf::from("source.txt"),
        content: "snapshot-sized content".to_string(),
        encoding: TextEncoding::Utf8,
        line_ending: LineEnding::Lf,
        snapshot: Some(file_snapshot(saved_byte_len)),
        read_only_reason: Some(source_reason),
    });

    app.mark_current_saved(
        PathBuf::from(format!("saved-{saved_byte_len}.txt")),
        TextEncoding::Utf8,
        LineEnding::Lf,
        Some(file_snapshot(saved_byte_len)),
    )
    .expect("mark save-as complete");

    assert_eq!(
        app.current_document()
            .and_then(|document| document.read_only_reason()),
        expected_reason
    );

    let status = app.current_editor_status();
    assert_eq!(status.is_read_only, expected_reason.is_some());
    assert_eq!(status.can_edit, expected_reason.is_none());
    assert_eq!(status.can_save, expected_reason.is_none());
    assert!(status.can_save_as);
}

fn file_snapshot(byte_len: u64) -> FileSnapshot {
    FileSnapshot {
        modified: None,
        byte_len,
    }
}

fn write_large_text_file(path: &Path, target_bytes: usize) {
    let file = File::create(path).expect("create large test file");
    let mut writer = BufWriter::new(file);
    let lines = [
        "alpha NEEDLE 한글 日本語\r\n",
        "plain line without match\n",
        "carriage return NEEDLE only\r",
        "long segment xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n",
    ];
    let mut written = 0usize;
    while written < target_bytes {
        for line in lines {
            if written >= target_bytes {
                break;
            }
            writer.write_all(line.as_bytes()).expect("write line");
            written += line.len();
        }
    }
    writer.flush().expect("flush large test file");
}

fn write_ascii_file(path: &Path, target_bytes: u64) {
    let file = File::create(path).expect("create ascii test file");
    let mut writer = BufWriter::new(file);
    let chunk = [b'a'; 8192];
    let mut remaining = target_bytes;
    while remaining > 0 {
        let chunk_len = remaining.min(chunk.len() as u64) as usize;
        writer
            .write_all(&chunk[..chunk_len])
            .expect("write ascii chunk");
        remaining -= chunk_len as u64;
    }
    writer.flush().expect("flush ascii test file");
}
