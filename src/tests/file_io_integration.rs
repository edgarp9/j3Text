use std::fs;
use std::path::{Path, PathBuf};

use j3text::app::EditorApp;
use j3text::domain::{LineEnding, TextEncoding};
use j3text::infra::{FileDocumentIo, UserDataStore};

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(prefix: &str) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
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
fn recent_files_round_trip_preserves_persisted_order() {
    let root = TempRoot::new("j3text-recent-files-order");
    let first = root.path().join("first.txt");
    let second = root.path().join("second.txt");

    let store = UserDataStore::with_root(root.path().join("user-data")).expect("create store");
    store
        .save_recent_files(&[first.clone(), second.clone()])
        .expect("save recent files");

    let loaded_recent_files = store.load_recent_files().expect("load recent files");
    let mut restored = EditorApp::new();
    restored.set_recent_files(loaded_recent_files);

    assert_eq!(restored.recent_files(), &[first, second]);
}

#[test]
fn save_as_new_file_then_reload_preserves_bytes_and_snapshot() {
    let root = TempRoot::new("j3text-save-as-reload");
    let io = FileDocumentIo::new();
    let path = root.path().join("draft.txt");

    let snapshot = io
        .save(
            &path,
            "alpha\nbeta",
            TextEncoding::Utf8,
            LineEnding::Lf,
            None,
        )
        .expect("save as new file");
    let saved_bytes = fs::read(&path).expect("read saved bytes");
    let reloaded = io.load(&path, None, None).expect("reload saved file");

    assert_eq!(saved_bytes, b"alpha\nbeta");
    assert_eq!(snapshot.byte_len, saved_bytes.len() as u64);
    assert_eq!(reloaded.content, "alpha\nbeta");
    assert_eq!(reloaded.snapshot, Some(snapshot));
}
