use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use j3text::app::EditorApp;
use j3text::domain::{EditorSettings, LineEnding, LoadedDocument, ReadOnlyReason, TextEncoding};
use j3text::infra::{FileDocumentIo, UserDataStore};

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
fn new_document_save_open_preserves_mixed_plain_text_and_crlf_line_breaks() {
    let root = TempRoot::new("j3text-plain-text-mixed");
    let io = FileDocumentIo::new();
    let path = root.path().join("mixed.txt");
    let content = "한글 English 123 !@#$%^&*()[]{}<> \"'\\\r\nsecond line\r\n{\\rtf1\\b literal}";

    let mut app = EditorApp::new();
    app.new_document();
    let document = app.current_document().expect("new document");
    assert_eq!(document.content(), "");
    assert_eq!(document.encoding(), TextEncoding::Utf8);
    assert_eq!(document.line_ending(), LineEnding::Crlf);
    assert!(!document.is_dirty());

    app.update_current_content(content.to_string())
        .expect("edit new document");
    let document = app.current_document().expect("edited document");
    assert!(document.is_dirty());
    assert_eq!(document.content(), content);
    let encoding = document.encoding();
    let line_ending = document.line_ending();

    let snapshot = io
        .save(&path, document.content(), encoding, line_ending, None)
        .expect("save mixed plain text");
    app.mark_current_saved(path.clone(), encoding, line_ending, Some(snapshot))
        .expect("mark saved");

    assert_eq!(
        fs::read(&path).expect("read saved bytes"),
        content.as_bytes()
    );
    assert!(!app.current_document().expect("saved document").is_dirty());

    let loaded = io.load(&path, None, None).expect("open saved file");
    assert_eq!(loaded.content, content);
    assert_eq!(loaded.encoding, TextEncoding::Utf8);
    assert_eq!(loaded.line_ending, LineEnding::Crlf);
}

#[test]
fn empty_document_save_open_stays_empty_and_clean() {
    let root = TempRoot::new("j3text-plain-text-empty");
    let io = FileDocumentIo::new();
    let path = root.path().join("empty.txt");
    let mut app = EditorApp::new();

    app.new_document();
    let document = app.current_document().expect("empty document");
    let encoding = document.encoding();
    let line_ending = document.line_ending();
    let snapshot = io
        .save(&path, document.content(), encoding, line_ending, None)
        .expect("save empty document");
    app.mark_current_saved(path.clone(), encoding, line_ending, Some(snapshot))
        .expect("mark empty document saved");

    assert_eq!(fs::metadata(&path).expect("empty metadata").len(), 0);
    assert!(
        !app.current_document()
            .expect("saved empty document")
            .is_dirty()
    );
    assert_eq!(
        io.load(&path, None, None).expect("open empty file").content,
        ""
    );
}

#[test]
fn chunk_sized_plain_text_save_open_preserves_large_document_lines() {
    let root = TempRoot::new("j3text-plain-text-large");
    let io = FileDocumentIo::new();
    let path = root.path().join("large.txt");
    let mut content = String::new();
    for index in 0..4096 {
        content.push_str("한글 English 123 symbols !@# line ");
        content.push_str(&index.to_string());
        content.push_str("\r\n");
    }
    let expected = LineEnding::Lf.normalize_text(&content);

    let snapshot = io
        .save(&path, &content, TextEncoding::Utf8, LineEnding::Lf, None)
        .expect("save chunk-sized plain text");
    let loaded = io.load(&path, None, None).expect("open chunk-sized file");

    assert!(snapshot.byte_len > 64 * 1024);
    assert_eq!(loaded.content, expected);
    assert_eq!(loaded.encoding, TextEncoding::Utf8);
    assert_eq!(loaded.line_ending, LineEnding::Lf);
}

#[test]
fn open_save_reopen_preserves_lf_policy_after_rich_edit_crlf_sync() {
    let root = TempRoot::new("j3text-plain-text-lf-policy");
    let io = FileDocumentIo::new();
    let path = root.path().join("lf.txt");
    fs::write(&path, b"first\nsecond\nthird").expect("write lf file");

    let loaded = io.load(&path, None, None).expect("open lf file");
    assert_eq!(loaded.content, "first\nsecond\nthird");
    assert_eq!(loaded.line_ending, LineEnding::Lf);

    let mut app = EditorApp::new();
    app.open_document(loaded);

    app.update_current_content("first\r\nsecond changed\r\nthird".to_string())
        .expect("sync edited Rich Edit text");
    let document = app.current_document().expect("edited document");
    let encoding = document.encoding();
    let line_ending = document.line_ending();
    let snapshot = io
        .save(
            &path,
            document.content(),
            encoding,
            line_ending,
            document.snapshot(),
        )
        .expect("save lf policy after crlf sync");
    app.mark_current_saved(path.clone(), encoding, line_ending, Some(snapshot))
        .expect("mark saved");

    assert_eq!(
        fs::read(&path).expect("read saved lf bytes"),
        b"first\nsecond changed\nthird"
    );

    let reopened = io.load(&path, None, None).expect("reopen lf file");
    assert_eq!(reopened.content, "first\nsecond changed\nthird");
    assert_eq!(reopened.line_ending, LineEnding::Lf);
}

#[test]
fn read_only_document_rejects_app_text_mutation_and_dirty_marking() {
    let mut app = EditorApp::new();
    app.open_document(LoadedDocument {
        path: PathBuf::from("readonly.txt"),
        content: "locked plain text".to_string(),
        encoding: TextEncoding::Utf8,
        line_ending: LineEnding::Crlf,
        snapshot: None,
        read_only_reason: Some(ReadOnlyReason::FileAttribute),
    });

    let document = app.current_document().expect("read-only document");
    assert!(document.is_read_only());
    assert!(!document.is_dirty());
    assert_eq!(document.content(), "locked plain text");

    app.update_current_content("mutated".to_string())
        .expect("attempt app mutation");
    app.mark_current_dirty_from_view()
        .expect("attempt view dirty mark");

    let document = app
        .current_document()
        .expect("read-only document after mutation attempts");
    assert_eq!(document.content(), "locked plain text");
    assert_eq!(
        document.read_only_reason(),
        Some(ReadOnlyReason::FileAttribute)
    );
    assert!(!document.is_dirty());
    assert!(document.tab_title().contains("[Read-only]"));
}

#[test]
fn word_wrap_setting_round_trip_does_not_change_document_or_saved_bytes() {
    let root = TempRoot::new("j3text-word-wrap-regression");
    let io = FileDocumentIo::new();
    let path = root.path().join("wrapped.txt");
    let content =
        "very long first line that may wrap in the UI but must stay one line\nsecond line";
    let mut app = EditorApp::new();

    app.new_document();
    app.update_current_content(content.to_string())
        .expect("edit document");
    let document = app.current_document().expect("edited document");
    let encoding = document.encoding();
    let snapshot = io
        .save(&path, document.content(), encoding, LineEnding::Lf, None)
        .expect("save document");
    app.mark_current_saved(path.clone(), encoding, LineEnding::Lf, Some(snapshot))
        .expect("mark saved");

    let settings = EditorSettings {
        word_wrap: true,
        ..EditorSettings::default()
    };
    app.set_settings(settings);

    let document = app.current_document().expect("wrapped document");
    assert_eq!(document.content(), content);
    assert_eq!(
        fs::read(&path).expect("read saved bytes"),
        content.as_bytes()
    );
    assert!(!document.is_dirty());

    let store =
        UserDataStore::with_root(root.path().join("user-data")).expect("create user data store");
    store
        .save_settings(app.settings())
        .expect("save word wrap setting");
    assert!(
        store
            .load_settings()
            .expect("load word wrap setting")
            .word_wrap
    );
}
