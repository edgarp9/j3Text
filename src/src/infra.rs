use std::env;
use std::fs::{self, File};
use std::io::{self, BufWriter, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(all(unix, not(windows)))]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

#[cfg(windows)]
use windows_sys::Win32::Foundation::HANDLE;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    FILE_BASIC_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileBasicInfo,
    GetFileInformationByHandleEx,
};

use crate::domain::{
    DocumentMetrics, EditorCommandId, EditorSettings, FileSnapshot, KeyboardShortcut, LineEnding,
    LoadedDocument, LoadedTextAnalysis, MAX_DOCUMENT_LOAD_BYTES, ReadOnlyReason, SavePolicy,
    SavePolicyError, TextEncoding, ThemeMode, can_load_document_bytes,
};
use crate::error::{AppError, FileAccessKind};
use crate::platform;

pub struct FileDocumentIo;

pub(crate) struct LoadedDocumentWithMetadata {
    pub(crate) document: LoadedDocument,
    pub(crate) metrics: DocumentMetrics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SavedFileSnapshot {
    pub(crate) snapshot: FileSnapshot,
    pub(crate) metadata: FileMetadataSnapshot,
}

// Confirmed large files still become one in-memory document, so keep a hard read ceiling.
pub const MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveTargetExpectation {
    Any,
    Unchanged(FileSnapshot),
    UnchangedWithMetadata {
        snapshot: FileSnapshot,
        metadata: FileMetadataSnapshot,
    },
    UnchangedMetadata(FileMetadataSnapshot),
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitialTargetCheck {
    Required,
    Skipped,
}

impl SaveTargetExpectation {
    fn from_snapshot(expected_snapshot: Option<FileSnapshot>) -> Self {
        match expected_snapshot {
            Some(snapshot) => Self::Unchanged(snapshot),
            None => Self::Any,
        }
    }

    fn target_must_exist(self) -> bool {
        matches!(
            self,
            Self::Unchanged(_) | Self::UnchangedWithMetadata { .. } | Self::UnchangedMetadata(_)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileMetadataSnapshot {
    modified: Option<SystemTime>,
    byte_len: u64,
    change_time: Option<i64>,
}

impl FileMetadataSnapshot {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            modified: metadata.modified().ok(),
            byte_len: metadata.len(),
            change_time: None,
        }
    }

    fn from_file_metadata(file: &File, metadata: &fs::Metadata) -> Self {
        Self {
            change_time: file_change_time(file),
            ..Self::from_metadata(metadata)
        }
    }

    pub fn byte_len(self) -> u64 {
        self.byte_len
    }

    pub fn has_changed_from(self, current: Self) -> bool {
        self.byte_len != current.byte_len || self.modified != current.modified
    }

    fn has_save_target_changed_from(self, current: Self) -> bool {
        self.byte_len != current.byte_len
            || self.modified != current.modified
            || self.has_change_marker_changed_from(current)
    }

    pub(crate) fn is_confirmed_unchanged_from(self, expected: Self) -> bool {
        self.byte_len == expected.byte_len
            && self.modified == expected.modified
            && change_marker_confirms_unchanged(self.change_time, expected.change_time)
    }

    #[cfg(test)]
    pub(crate) fn has_change_marker(self) -> bool {
        self.change_time.is_some()
    }

    pub(crate) fn has_change_marker_changed_from(self, current: Self) -> bool {
        matches!(
            (self.change_time, current.change_time),
            (Some(previous), Some(current)) if previous != current
        )
    }
}

#[cfg(windows)]
fn change_marker_confirms_unchanged(current: Option<i64>, expected: Option<i64>) -> bool {
    matches!(
        (current, expected),
        (Some(current), Some(expected)) if current == expected
    )
}

#[cfg(not(windows))]
fn change_marker_confirms_unchanged(_current: Option<i64>, _expected: Option<i64>) -> bool {
    false
}

impl FileDocumentIo {
    pub fn new() -> Self {
        Self
    }

    pub fn load(
        &self,
        path: &Path,
        requested_encoding: Option<TextEncoding>,
        forced_read_only_reason: Option<ReadOnlyReason>,
    ) -> Result<LoadedDocument, AppError> {
        document_repository::load(path, requested_encoding, forced_read_only_reason)
    }

    pub(crate) fn load_with_metadata_and_prechecked_len(
        &self,
        path: &Path,
        requested_encoding: Option<TextEncoding>,
        forced_read_only_reason: Option<ReadOnlyReason>,
        prechecked_byte_len: u64,
    ) -> Result<LoadedDocumentWithMetadata, AppError> {
        document_repository::load_with_metadata_and_prechecked_len(
            path,
            requested_encoding,
            forced_read_only_reason,
            prechecked_byte_len,
        )
    }

    pub fn save(
        &self,
        path: &Path,
        content: &str,
        encoding: TextEncoding,
        line_ending: LineEnding,
        expected_snapshot: Option<FileSnapshot>,
    ) -> Result<FileSnapshot, AppError> {
        self.save_with_target_expectation(
            path,
            content,
            encoding,
            line_ending,
            SaveTargetExpectation::from_snapshot(expected_snapshot),
        )
    }

    pub fn save_with_target_expectation(
        &self,
        path: &Path,
        content: &str,
        encoding: TextEncoding,
        line_ending: LineEnding,
        target_expectation: SaveTargetExpectation,
    ) -> Result<FileSnapshot, AppError> {
        Ok(self
            .save_with_target_expectation_and_metadata(
                path,
                content,
                encoding,
                line_ending,
                target_expectation,
            )?
            .snapshot)
    }

    pub(crate) fn save_with_target_expectation_and_metadata(
        &self,
        path: &Path,
        content: &str,
        encoding: TextEncoding,
        line_ending: LineEnding,
        target_expectation: SaveTargetExpectation,
    ) -> Result<SavedFileSnapshot, AppError> {
        document_repository::save(path, content, encoding, line_ending, target_expectation)
    }

    pub fn ensure_encodable(&self, content: &str, encoding: TextEncoding) -> Result<(), AppError> {
        text_codec::ensure_encodable(content, encoding)
    }

    pub fn file_snapshot(&self, path: &Path) -> Result<FileSnapshot, AppError> {
        document_repository::file_snapshot(path)
    }

    pub fn file_metadata_snapshot(&self, path: &Path) -> Result<FileMetadataSnapshot, AppError> {
        document_repository::file_metadata_snapshot(path)
    }

    pub fn file_byte_len(&self, path: &Path) -> Result<u64, AppError> {
        document_repository::file_byte_len(path)
    }
}

mod document_repository {
    use super::*;

    pub(super) fn load(
        path: &Path,
        requested_encoding: Option<TextEncoding>,
        forced_read_only_reason: Option<ReadOnlyReason>,
    ) -> Result<LoadedDocument, AppError> {
        Ok(load_with_metadata(path, requested_encoding, forced_read_only_reason)?.document)
    }

    pub(super) fn load_with_metadata(
        path: &Path,
        requested_encoding: Option<TextEncoding>,
        forced_read_only_reason: Option<ReadOnlyReason>,
    ) -> Result<LoadedDocumentWithMetadata, AppError> {
        let read = read_document_bytes(path, forced_read_only_reason, None)?;
        load_with_read_bytes(path, requested_encoding, forced_read_only_reason, read)
    }

    pub(super) fn load_with_metadata_and_prechecked_len(
        path: &Path,
        requested_encoding: Option<TextEncoding>,
        forced_read_only_reason: Option<ReadOnlyReason>,
        prechecked_byte_len: u64,
    ) -> Result<LoadedDocumentWithMetadata, AppError> {
        let read = read_document_bytes(path, forced_read_only_reason, Some(prechecked_byte_len))?;
        load_with_read_bytes(path, requested_encoding, forced_read_only_reason, read)
    }

    fn load_with_read_bytes(
        path: &Path,
        requested_encoding: Option<TextEncoding>,
        forced_read_only_reason: Option<ReadOnlyReason>,
        read: ReadDocumentBytes,
    ) -> Result<LoadedDocumentWithMetadata, AppError> {
        let file_read_only = read.file_read_only;
        let (content, encoding) =
            text_codec::decode_document_bytes(read.bytes, requested_encoding)?;
        let text_scan = LoadedTextAnalysis::scan_text(&content);
        reject_nul_presence(text_scan.contains_nul)?;
        let text_analysis = text_scan.analysis;
        let snapshot = Some(FileSnapshot {
            modified: snapshot_modified_marker(read.modified, read.content_fingerprint),
            byte_len: read.byte_len,
        });
        let read_only_reason = if file_read_only {
            Some(ReadOnlyReason::FileAttribute)
        } else {
            forced_read_only_reason
        };

        let document = LoadedDocument {
            path: path.to_path_buf(),
            content,
            encoding,
            line_ending: text_analysis.line_ending,
            snapshot,
            read_only_reason,
        };

        Ok(LoadedDocumentWithMetadata {
            document,
            metrics: text_analysis.metrics,
        })
    }

    pub(super) fn save(
        path: &Path,
        content: &str,
        encoding: TextEncoding,
        line_ending: LineEnding,
        target_expectation: SaveTargetExpectation,
    ) -> Result<SavedFileSnapshot, AppError> {
        if let Ok(metadata) = fs::metadata(path)
            && metadata.permissions().readonly()
        {
            return Err(AppError::file_access(
                FileAccessKind::ReadOnly,
                io::Error::new(ErrorKind::PermissionDenied, "target file is read-only"),
                "save file",
                Some(path.to_path_buf()),
            ));
        }

        let written = atomic_writer::write_encoded_text(
            path,
            content,
            encoding,
            line_ending,
            target_expectation,
            InitialTargetCheck::Skipped,
        )?;
        saved_file_snapshot(path, written)
    }

    pub(super) fn file_snapshot(path: &Path) -> Result<FileSnapshot, AppError> {
        super::file_snapshot(path)
    }

    pub(super) fn file_metadata_snapshot(path: &Path) -> Result<FileMetadataSnapshot, AppError> {
        super::file_metadata_snapshot(path)
    }

    pub(super) fn file_byte_len(path: &Path) -> Result<u64, AppError> {
        super::file_byte_len(path)
    }
}

fn file_metadata_snapshot(path: &Path) -> Result<FileMetadataSnapshot, AppError> {
    if let Ok(file) = open_file_for_metadata(path)
        && let Ok(metadata) = file.metadata()
    {
        return Ok(FileMetadataSnapshot::from_file_metadata(&file, &metadata));
    }

    let metadata = fs::metadata(path)
        .map_err(|source| AppError::io_path(source, "read metadata", path.to_path_buf()))?;
    Ok(FileMetadataSnapshot::from_metadata(&metadata))
}

fn file_byte_len(path: &Path) -> Result<u64, AppError> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|source| AppError::io_path(source, "read metadata", path.to_path_buf()))
}

#[cfg(windows)]
fn open_file_for_metadata(path: &Path) -> io::Result<File> {
    fs::OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(path)
}

#[cfg(not(windows))]
fn open_file_for_metadata(path: &Path) -> io::Result<File> {
    File::open(path)
}

#[cfg(windows)]
fn file_change_time(file: &File) -> Option<i64> {
    let mut info = FILE_BASIC_INFO::default();
    let buffer_size = u32::try_from(std::mem::size_of::<FILE_BASIC_INFO>()).ok()?;
    let ok = unsafe {
        // SAFETY: info is a valid writable FILE_BASIC_INFO buffer for this call,
        // and the File handle remains open for the duration of the query.
        GetFileInformationByHandleEx(
            file.as_raw_handle() as HANDLE,
            FileBasicInfo,
            std::ptr::addr_of_mut!(info).cast(),
            buffer_size,
        )
    };
    (ok != 0).then_some(info.ChangeTime)
}

#[cfg(all(unix, not(windows)))]
fn file_change_time(file: &File) -> Option<i64> {
    let metadata = file.metadata().ok()?;
    metadata
        .ctime()
        .checked_mul(1_000_000_000)
        .and_then(|seconds| seconds.checked_add(metadata.ctime_nsec()))
}

#[cfg(not(any(windows, unix)))]
fn file_change_time(_file: &File) -> Option<i64> {
    None
}

fn file_snapshot(path: &Path) -> Result<FileSnapshot, AppError> {
    read_file_content_snapshot(path, MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES)
        .map(FileContentSnapshot::into_file_snapshot)
}

fn saved_file_snapshot(
    path: &Path,
    saved: SavedContentFingerprint,
) -> Result<SavedFileSnapshot, AppError> {
    let current = file_metadata_snapshot(path).map_err(|error| {
        if error.file_access_kind() == Some(FileAccessKind::NotFound) {
            AppError::external_file_changed(path.to_path_buf())
        } else {
            error
        }
    })?;
    if current.has_changed_from(saved.metadata) || current.byte_len != saved.written.byte_len {
        return Err(AppError::external_file_changed(path.to_path_buf()));
    }
    Ok(SavedFileSnapshot {
        snapshot: FileSnapshot {
            modified: snapshot_modified_marker(current.modified, saved.written.content_fingerprint),
            byte_len: saved.written.byte_len,
        },
        metadata: current,
    })
}

fn read_file_content_snapshot(
    path: &Path,
    read_limit: u64,
) -> Result<FileContentSnapshot, AppError> {
    let mut file = File::open(path)
        .map_err(|source| AppError::io_path(source, "open file", path.to_path_buf()))?;
    let initial_metadata = file
        .metadata()
        .map_err(|source| AppError::io_path(source, "read metadata", path.to_path_buf()))?;
    if initial_metadata.len() > read_limit {
        return Err(AppError::file_too_large(
            path.to_path_buf(),
            initial_metadata.len(),
            read_limit,
        ));
    }

    let content = content_fingerprint_from_reader(&mut file, path, read_limit)?;
    let metadata = file
        .metadata()
        .map_err(|source| AppError::io_path(source, "read metadata", path.to_path_buf()))?;
    if metadata.len() > read_limit {
        return Err(AppError::file_too_large(
            path.to_path_buf(),
            metadata.len(),
            read_limit,
        ));
    }

    Ok(FileContentSnapshot {
        modified: metadata.modified().ok(),
        content_fingerprint: content.content_fingerprint,
        byte_len: content.byte_len,
    })
}

fn ensure_target_snapshot_matches(
    path: &Path,
    expected_snapshot: Option<FileSnapshot>,
    expected_metadata: Option<FileMetadataSnapshot>,
) -> Result<(), AppError> {
    let Some(expected) = expected_snapshot else {
        return Ok(());
    };

    let current_metadata = file_metadata_snapshot(path).map_err(|error| {
        if error.file_access_kind() == Some(FileAccessKind::NotFound) {
            AppError::external_file_changed(path.to_path_buf())
        } else {
            error
        }
    })?;

    if expected.byte_len != current_metadata.byte_len {
        return Err(AppError::external_file_changed(path.to_path_buf()));
    }

    if current_metadata.byte_len > MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES {
        return Err(AppError::external_file_changed(path.to_path_buf()));
    }

    if let Some(expected_metadata) = expected_metadata {
        if expected_metadata.has_save_target_changed_from(current_metadata) {
            return Err(AppError::external_file_changed(path.to_path_buf()));
        }
        if current_metadata.is_confirmed_unchanged_from(expected_metadata) {
            return Ok(());
        }
    }

    let current = read_file_content_snapshot(path, MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES)
        .map_err(|error| {
            if error.file_access_kind() == Some(FileAccessKind::NotFound)
                || matches!(&error, AppError::FileTooLarge { .. })
            {
                AppError::external_file_changed(path.to_path_buf())
            } else {
                error
            }
        })?
        .into_file_snapshot();
    if expected.has_changed_from(current) {
        return Err(AppError::external_file_changed(path.to_path_buf()));
    }
    Ok(())
}

fn ensure_target_expectation_matches(
    path: &Path,
    target_expectation: SaveTargetExpectation,
) -> Result<(), AppError> {
    match target_expectation {
        SaveTargetExpectation::Any => Ok(()),
        SaveTargetExpectation::Unchanged(snapshot) => {
            ensure_target_snapshot_matches(path, Some(snapshot), None)
        }
        SaveTargetExpectation::UnchangedWithMetadata { snapshot, metadata } => {
            ensure_target_snapshot_matches(path, Some(snapshot), Some(metadata))
        }
        SaveTargetExpectation::UnchangedMetadata(metadata) => {
            let current_metadata = file_metadata_snapshot(path).map_err(|error| {
                if error.file_access_kind() == Some(FileAccessKind::NotFound) {
                    AppError::external_file_changed(path.to_path_buf())
                } else {
                    error
                }
            })?;
            if metadata.has_save_target_changed_from(current_metadata) {
                Err(AppError::external_file_changed(path.to_path_buf()))
            } else {
                Ok(())
            }
        }
        SaveTargetExpectation::Missing => match fs::metadata(path) {
            Ok(_) => Err(AppError::external_file_changed(path.to_path_buf())),
            Err(source) if source.kind() == ErrorKind::NotFound => Ok(()),
            Err(source) => Err(AppError::io_path(
                source,
                "read metadata",
                path.to_path_buf(),
            )),
        },
    }
}

fn content_fingerprint_from_reader(
    file: &mut File,
    path: &Path,
    read_limit: u64,
) -> Result<WrittenContentFingerprint, AppError> {
    #[cfg(test)]
    note_file_fingerprint_read();

    let mut content_fingerprint = CONTENT_FINGERPRINT_OFFSET;
    let mut byte_len = 0_u64;
    let mut buffer = [0_u8; WRITE_CHUNK_BYTES];

    loop {
        let remaining = read_limit.saturating_add(1).saturating_sub(byte_len);
        if remaining == 0 {
            break;
        }
        let read_len = buffer.len().min(remaining as usize);
        let read = file.read(&mut buffer[..read_len]).map_err(|source| {
            AppError::io_path(source, "read file fingerprint", path.to_path_buf())
        })?;
        if read == 0 {
            break;
        }
        content_fingerprint = update_content_fingerprint(content_fingerprint, &buffer[..read]);
        byte_len = byte_len.saturating_add(read as u64);
        if byte_len > read_limit {
            let observed_len = file
                .metadata()
                .map(|metadata| metadata.len())
                .unwrap_or(byte_len)
                .max(byte_len);
            return Err(AppError::file_too_large(
                path.to_path_buf(),
                observed_len,
                read_limit,
            ));
        }
    }

    Ok(WrittenContentFingerprint {
        content_fingerprint,
        byte_len,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WrittenContentFingerprint {
    content_fingerprint: u64,
    byte_len: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SavedContentFingerprint {
    written: WrittenContentFingerprint,
    metadata: FileMetadataSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileContentSnapshot {
    modified: Option<SystemTime>,
    content_fingerprint: u64,
    byte_len: u64,
}

impl FileContentSnapshot {
    fn into_file_snapshot(self) -> FileSnapshot {
        FileSnapshot {
            modified: snapshot_modified_marker(self.modified, self.content_fingerprint),
            byte_len: self.byte_len,
        }
    }
}

#[derive(Debug)]
struct ReadDocumentBytes {
    bytes: Vec<u8>,
    content_fingerprint: u64,
    byte_len: u64,
    modified: Option<SystemTime>,
    file_read_only: bool,
}

#[cfg(test)]
thread_local! {
    static FILE_FINGERPRINT_READ_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn note_file_fingerprint_read() {
    FILE_FINGERPRINT_READ_COUNT.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
pub(crate) fn reset_file_fingerprint_read_count() {
    FILE_FINGERPRINT_READ_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn file_fingerprint_read_count() -> u64 {
    FILE_FINGERPRINT_READ_COUNT.with(std::cell::Cell::get)
}

#[cfg(test)]
fn content_fingerprint(bytes: &[u8]) -> u64 {
    update_content_fingerprint(CONTENT_FINGERPRINT_OFFSET, bytes)
}

fn update_content_fingerprint(mut state: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(CONTENT_FINGERPRINT_PRIME);
    }
    state
}

const CONTENT_FINGERPRINT_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const CONTENT_FINGERPRINT_PRIME: u64 = 0x0000_0100_0000_01b3;
const FINGERPRINT_MARKER_SECONDS: u64 = 24 * 60 * 60;

fn snapshot_modified_marker(
    modified: Option<SystemTime>,
    content_fingerprint: u64,
) -> Option<SystemTime> {
    let base = modified.unwrap_or(UNIX_EPOCH);
    let marker = Duration::new(
        content_fingerprint % FINGERPRINT_MARKER_SECONDS,
        ((content_fingerprint >> 32) % 1_000_000_000) as u32,
    );
    base.checked_add(marker).or(Some(base))
}

#[cfg(windows)]
fn open_target_save_guard(
    path: &Path,
    target_expectation: SaveTargetExpectation,
) -> Result<Option<File>, AppError> {
    if !target_expectation.target_must_exist() {
        return Ok(None);
    }

    fs::OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .open(path)
        .map(Some)
        .map_err(|source| {
            if source.kind() == ErrorKind::NotFound {
                AppError::external_file_changed(path.to_path_buf())
            } else {
                AppError::io_path(source, "open target save guard", path.to_path_buf())
            }
        })
}

#[cfg(not(windows))]
fn open_target_save_guard(
    path: &Path,
    target_expectation: SaveTargetExpectation,
) -> Result<Option<File>, AppError> {
    let _ = (path, target_expectation);
    Ok(None)
}

fn read_document_bytes(
    path: &Path,
    forced_read_only_reason: Option<ReadOnlyReason>,
    prechecked_byte_len: Option<u64>,
) -> Result<ReadDocumentBytes, AppError> {
    let file = File::open(path)
        .map_err(|source| AppError::io_path(source, "open file", path.to_path_buf()))?;
    let metadata_byte_len = match prechecked_byte_len {
        Some(byte_len) => byte_len,
        None => file
            .metadata()
            .map_err(|source| AppError::io_path(source, "read metadata", path.to_path_buf()))?
            .len(),
    };
    let read_limit = if forced_read_only_reason == Some(ReadOnlyReason::LargeFile) {
        if metadata_byte_len > MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES {
            return Err(AppError::file_too_large(
                path.to_path_buf(),
                metadata_byte_len,
                MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES,
            ));
        }
        MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES
    } else {
        if !can_load_document_bytes(metadata_byte_len) {
            return Err(AppError::file_too_large(
                path.to_path_buf(),
                metadata_byte_len,
                MAX_DOCUMENT_LOAD_BYTES,
            ));
        }
        MAX_DOCUMENT_LOAD_BYTES
    };

    let capacity = usize::try_from(metadata_byte_len.min(read_limit)).unwrap_or(usize::MAX);
    let mut bytes = Vec::with_capacity(capacity);
    let mut limited = file.take(read_limit.saturating_add(1));
    let mut buffer = [0_u8; WRITE_CHUNK_BYTES];
    let mut content_fingerprint = CONTENT_FINGERPRINT_OFFSET;

    loop {
        let read = limited
            .read(&mut buffer)
            .map_err(|source| AppError::io_path(source, "read file", path.to_path_buf()))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        content_fingerprint = update_content_fingerprint(content_fingerprint, &buffer[..read]);
        if bytes.len() as u64 > read_limit {
            break;
        }
    }

    if bytes.len() as u64 > read_limit {
        let observed_len = fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(bytes.len() as u64)
            .max(bytes.len() as u64);
        return Err(AppError::file_too_large(
            path.to_path_buf(),
            observed_len,
            read_limit,
        ));
    }

    let read_metadata = limited
        .get_ref()
        .metadata()
        .map_err(|source| AppError::io_path(source, "read metadata", path.to_path_buf()))?;
    let byte_len = bytes.len() as u64;
    let modified = read_metadata.modified().ok();

    Ok(ReadDocumentBytes {
        bytes,
        content_fingerprint,
        byte_len,
        modified,
        file_read_only: read_metadata.permissions().readonly(),
    })
}

fn read_user_data_text(path: &Path, action: &'static str, limit: u64) -> Result<String, AppError> {
    let file =
        File::open(path).map_err(|source| AppError::io_path(source, action, path.to_path_buf()))?;
    let metadata = file
        .metadata()
        .map_err(|source| AppError::io_path(source, action, path.to_path_buf()))?;
    let byte_len = metadata.len();
    if byte_len > limit {
        return Err(AppError::file_too_large(
            path.to_path_buf(),
            byte_len,
            limit,
        ));
    }

    let mut bytes = usize::try_from(byte_len).map_or_else(|_| Vec::new(), Vec::with_capacity);
    let mut limited = file.take(limit.saturating_add(1));
    limited
        .read_to_end(&mut bytes)
        .map_err(|source| AppError::io_path(source, action, path.to_path_buf()))?;

    if bytes.len() as u64 > limit {
        let observed_len = fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(bytes.len() as u64)
            .max(bytes.len() as u64);
        return Err(AppError::file_too_large(
            path.to_path_buf(),
            observed_len,
            limit,
        ));
    }

    String::from_utf8(bytes).map_err(|source| {
        AppError::io_path(
            io::Error::new(ErrorKind::InvalidData, source),
            action,
            path.to_path_buf(),
        )
    })
}

impl Default for FileDocumentIo {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct UserDataStore {
    paths: UserDataPaths,
}

#[derive(Clone)]
struct UserDataPaths {
    user_data_root: PathBuf,
    settings_file: PathBuf,
}

impl UserDataStore {
    pub fn new() -> Result<Self, AppError> {
        user_data_store::new()
    }

    pub fn with_root(root: PathBuf) -> Result<Self, AppError> {
        user_data_store::with_root(root)
    }

    pub fn load_settings(&self) -> Result<EditorSettings, AppError> {
        user_data_store::load_settings(&self.paths.settings_file)
    }

    pub fn save_settings(&self, settings: &EditorSettings) -> Result<(), AppError> {
        user_data_store::save_settings(&self.paths.settings_file, settings)
    }

    pub fn load_recent_files(&self) -> Result<Vec<PathBuf>, AppError> {
        user_data_store::load_recent_files(&self.paths.user_data_root)
    }

    pub fn save_recent_files(&self, recent_files: &[PathBuf]) -> Result<(), AppError> {
        user_data_store::save_recent_files(&self.paths.user_data_root, recent_files)
    }

    #[cfg(test)]
    fn settings_path(&self) -> PathBuf {
        self.paths.settings_file.clone()
    }

    #[cfg(test)]
    fn user_data_root(&self) -> &Path {
        &self.paths.user_data_root
    }

    #[cfg(test)]
    fn recent_files_path(&self) -> PathBuf {
        user_data_store::recent_files_path(&self.paths.user_data_root)
    }
}

mod user_data_store {
    use super::*;

    pub(super) fn new() -> Result<UserDataStore, AppError> {
        with_paths(UserDataPaths {
            user_data_root: default_user_data_root()?,
            settings_file: executable_settings_path()?,
        })
    }

    pub(super) fn with_root(root: PathBuf) -> Result<UserDataStore, AppError> {
        let settings_file = root.join(default_settings_file_name());
        with_paths(UserDataPaths {
            user_data_root: root,
            settings_file,
        })
    }

    fn with_paths(paths: UserDataPaths) -> Result<UserDataStore, AppError> {
        fs::create_dir_all(&paths.user_data_root)
            .map_err(|source| AppError::io(source, "create user data dir"))?;
        Ok(UserDataStore { paths })
    }

    pub(super) fn load_settings(path: &Path) -> Result<EditorSettings, AppError> {
        let text = match read_user_data_text(path, "read settings", USER_DATA_TEXT_LOAD_LIMIT_BYTES)
        {
            Ok(text) => text,
            Err(error) if error.file_access_kind() == Some(FileAccessKind::NotFound) => {
                return Ok(EditorSettings::default());
            }
            Err(error) => return Err(error),
        };

        Ok(user_data_codec::settings::parse(&text))
    }

    pub(super) fn save_settings(path: &Path, settings: &EditorSettings) -> Result<(), AppError> {
        let text = user_data_codec::settings::format(settings);
        atomic_writer::write_text(path, &text)
    }

    pub(super) fn load_recent_files(root: &Path) -> Result<Vec<PathBuf>, AppError> {
        let path = recent_files_path(root);
        let text = match read_user_data_text(
            &path,
            "read recent files",
            USER_DATA_TEXT_LOAD_LIMIT_BYTES,
        ) {
            Ok(text) => text,
            Err(error) if error.file_access_kind() == Some(FileAccessKind::NotFound) => {
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };

        Ok(user_data_codec::recent_files::parse(&text))
    }

    pub(super) fn save_recent_files(root: &Path, recent_files: &[PathBuf]) -> Result<(), AppError> {
        let text = user_data_codec::recent_files::format(recent_files);
        atomic_writer::write_text(&recent_files_path(root), &text)
    }

    pub(super) fn recent_files_path(root: &Path) -> PathBuf {
        root.join("recent-files.conf")
    }
}

fn default_user_data_root() -> Result<PathBuf, AppError> {
    let mut root = user_data_root_base()?;
    root.push("j3Text");
    Ok(root)
}

fn executable_settings_path() -> Result<PathBuf, AppError> {
    let executable_path =
        env::current_exe().map_err(|source| AppError::io(source, "resolve executable path"))?;
    settings_path_for_executable_path(executable_path)
}

fn default_settings_file_name() -> &'static str {
    concat!(env!("CARGO_PKG_NAME"), ".toml")
}

fn settings_path_for_executable_path(mut executable_path: PathBuf) -> Result<PathBuf, AppError> {
    if executable_path.file_name().is_none() {
        return Err(AppError::InvalidState("Executable path has no file name."));
    }
    executable_path.set_extension("toml");
    Ok(executable_path)
}

#[cfg(windows)]
fn user_data_root_base() -> Result<PathBuf, AppError> {
    if let Some(appdata) = env::var_os("APPDATA") {
        return Ok(PathBuf::from(appdata));
    }
    env::current_dir().map_err(|source| AppError::io(source, "resolve current dir"))
}

#[cfg(not(windows))]
fn user_data_root_base() -> Result<PathBuf, AppError> {
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(config_home));
    }
    if let Some(home) = env::var_os("HOME") {
        return Ok(PathBuf::from(home).join(".config"));
    }
    env::current_dir().map_err(|source| AppError::io(source, "resolve current dir"))
}

mod user_data_codec {
    use super::*;

    pub(super) mod settings {
        use super::*;

        pub(in crate::infra) fn parse(text: &str) -> EditorSettings {
            parse_toml_settings(text).unwrap_or_default().sanitized()
        }

        pub(in crate::infra) fn format(settings: &EditorSettings) -> String {
            let settings = settings.clone().sanitized();
            let mut table = toml::Table::new();
            table.insert("font_name".to_string(), settings.font_name.into());
            table.insert(
                "font_size_pt".to_string(),
                toml::Value::Integer(i64::from(settings.font_size_pt)),
            );
            table.insert(
                "tab_size".to_string(),
                toml::Value::Integer(i64::from(settings.tab_size)),
            );
            table.insert("word_wrap".to_string(), settings.word_wrap.into());
            table.insert(
                "show_whitespace".to_string(),
                settings.show_whitespace.into(),
            );
            table.insert("theme".to_string(), settings.theme.key().into());
            for command in EditorCommandId::SHORTCUT_COMMANDS {
                let Some(key) = command.shortcut_storage_key() else {
                    continue;
                };
                table.insert(
                    key.to_string(),
                    optional_shortcut_storage_key(settings.shortcuts.shortcut_for(command)).into(),
                );
            }

            let mut text = table.to_string();
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text
        }

        fn parse_toml_settings(text: &str) -> Option<EditorSettings> {
            let table = text.parse::<toml::Table>().ok()?;
            let mut settings = EditorSettings::default();

            if let Some(font_name) = table.get("font_name").and_then(toml::Value::as_str) {
                settings.font_name = font_name.to_string();
            }
            if let Some(size) = table
                .get("font_size_pt")
                .and_then(toml::Value::as_integer)
                .and_then(|size| u32::try_from(size).ok())
            {
                settings.font_size_pt = size;
            }
            if let Some(tab_size) = table
                .get("tab_size")
                .and_then(toml::Value::as_integer)
                .and_then(|tab_size| u8::try_from(tab_size).ok())
            {
                settings.tab_size = tab_size;
            }
            if let Some(word_wrap) = table.get("word_wrap").and_then(toml::Value::as_bool) {
                settings.word_wrap = word_wrap;
            }
            if let Some(show_whitespace) =
                table.get("show_whitespace").and_then(toml::Value::as_bool)
            {
                settings.show_whitespace = show_whitespace;
            }
            if let Some(theme) = table
                .get("theme")
                .and_then(toml::Value::as_str)
                .and_then(ThemeMode::from_key)
            {
                settings.theme = theme;
            }
            for command in EditorCommandId::SHORTCUT_COMMANDS {
                let Some(key) = command.shortcut_storage_key() else {
                    continue;
                };
                if let Some(shortcut) = table
                    .get(key)
                    .and_then(toml::Value::as_str)
                    .and_then(parse_optional_shortcut)
                {
                    settings.shortcuts.set_shortcut(command, shortcut);
                }
            }

            Some(settings)
        }

        fn parse_optional_shortcut(value: &str) -> Option<Option<KeyboardShortcut>> {
            if value.eq_ignore_ascii_case("none") {
                Some(None)
            } else {
                KeyboardShortcut::from_storage_key(value).map(Some)
            }
        }

        fn optional_shortcut_storage_key(shortcut: Option<KeyboardShortcut>) -> String {
            shortcut
                .map(KeyboardShortcut::storage_key)
                .unwrap_or_else(|| "none".to_string())
        }
    }

    pub(super) mod recent_files {
        use super::*;

        pub(in crate::infra) fn parse(text: &str) -> Vec<PathBuf> {
            let mut recent_files = Vec::new();
            for line in text.lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                if key == "recent" {
                    recent_files.push(path_value::unescape(value));
                }
            }

            recent_files
        }

        pub(in crate::infra) fn format(recent_files: &[PathBuf]) -> String {
            let mut text = String::new();
            for path in recent_files {
                text.push_str("recent=");
                text.push_str(&path_value::escape(path));
                text.push('\n');
            }
            text
        }
    }

    pub(super) mod path_value {
        use super::*;

        pub(super) fn escape(path: &Path) -> String {
            field::escape(&path_storage_value(path))
        }

        pub(super) fn unescape(value: &str) -> PathBuf {
            let value = field::unescape(value);
            path_from_storage_value(&value)
        }

        #[cfg(windows)]
        const WINDOWS_PATH_STORAGE_PREFIX: &str = "?j3path-win16:";

        #[cfg(windows)]
        fn path_storage_value(path: &Path) -> String {
            use std::os::windows::ffi::OsStrExt;

            let mut encoded = String::from(WINDOWS_PATH_STORAGE_PREFIX);
            for unit in path.as_os_str().encode_wide() {
                push_hex_u16(&mut encoded, unit);
            }
            encoded
        }

        #[cfg(windows)]
        fn path_from_storage_value(value: &str) -> PathBuf {
            if let Some(encoded) = value.strip_prefix(WINDOWS_PATH_STORAGE_PREFIX)
                && let Some(path) = parse_windows_path_storage_value(encoded)
            {
                return path;
            }

            PathBuf::from(value)
        }

        #[cfg(windows)]
        fn parse_windows_path_storage_value(encoded: &str) -> Option<PathBuf> {
            use std::ffi::OsString;
            use std::os::windows::ffi::OsStringExt;

            if !encoded.len().is_multiple_of(4) {
                return None;
            }

            let mut units = Vec::with_capacity(encoded.len() / 4);
            for chunk in encoded.as_bytes().chunks_exact(4) {
                let digits = std::str::from_utf8(chunk).ok()?;
                units.push(u16::from_str_radix(digits, 16).ok()?);
            }

            Some(PathBuf::from(OsString::from_wide(&units)))
        }

        #[cfg(windows)]
        fn push_hex_u16(output: &mut String, unit: u16) {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";

            for shift in [12, 8, 4, 0] {
                let index = ((unit >> shift) & 0xF) as usize;
                output.push(HEX[index] as char);
            }
        }

        #[cfg(unix)]
        const UNIX_PATH_STORAGE_PREFIX: &str = "?j3path-unix-bytes:";

        #[cfg(unix)]
        fn path_storage_value(path: &Path) -> String {
            use std::os::unix::ffi::OsStrExt;

            let mut encoded = String::from(UNIX_PATH_STORAGE_PREFIX);
            for byte in path.as_os_str().as_bytes() {
                push_hex_u8(&mut encoded, *byte);
            }
            encoded
        }

        #[cfg(unix)]
        fn path_from_storage_value(value: &str) -> PathBuf {
            if let Some(encoded) = value.strip_prefix(UNIX_PATH_STORAGE_PREFIX)
                && let Some(path) = parse_unix_path_storage_value(encoded)
            {
                return path;
            }

            PathBuf::from(value)
        }

        #[cfg(unix)]
        fn parse_unix_path_storage_value(encoded: &str) -> Option<PathBuf> {
            use std::ffi::OsString;
            use std::os::unix::ffi::OsStringExt;

            if !encoded.len().is_multiple_of(2) {
                return None;
            }

            let mut bytes = Vec::with_capacity(encoded.len() / 2);
            for chunk in encoded.as_bytes().chunks_exact(2) {
                let digits = std::str::from_utf8(chunk).ok()?;
                bytes.push(u8::from_str_radix(digits, 16).ok()?);
            }

            Some(PathBuf::from(OsString::from_vec(bytes)))
        }

        #[cfg(unix)]
        fn push_hex_u8(output: &mut String, byte: u8) {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";

            for shift in [4, 0] {
                let index = ((byte >> shift) & 0xF) as usize;
                output.push(HEX[index] as char);
            }
        }

        #[cfg(not(any(windows, unix)))]
        fn path_storage_value(path: &Path) -> String {
            path.as_os_str()
                .to_str()
                .map(ToOwned::to_owned)
                .unwrap_or_default()
        }

        #[cfg(not(any(windows, unix)))]
        fn path_from_storage_value(value: &str) -> PathBuf {
            PathBuf::from(value)
        }
    }

    pub(super) mod field {
        pub(in crate::infra) fn escape(value: &str) -> String {
            let mut escaped = String::with_capacity(value.len());
            for ch in value.chars() {
                match ch {
                    '%' => escaped.push_str("%25"),
                    '\r' => escaped.push_str("%0D"),
                    '\n' => escaped.push_str("%0A"),
                    _ => escaped.push(ch),
                }
            }
            escaped
        }

        pub(in crate::infra) fn unescape(value: &str) -> String {
            let mut decoded = String::with_capacity(value.len());
            let mut chars = value.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch != '%' {
                    decoded.push(ch);
                    continue;
                }

                let first = chars.next();
                let second = chars.next();
                let Some(first) = first else {
                    decoded.push('%');
                    break;
                };
                let Some(second) = second else {
                    decoded.push('%');
                    decoded.push(first);
                    break;
                };

                let hex = [first, second];
                let value = hex.iter().try_fold(0u8, |acc, digit| {
                    digit.to_digit(16).map(|n| acc * 16 + n as u8)
                });
                if let Some(byte) = value {
                    decoded.push(byte as char);
                } else {
                    decoded.push('%');
                    decoded.push(first);
                    decoded.push(second);
                }
            }
            decoded
        }
    }
}

#[cfg(test)]
use user_data_codec::field::{escape as escape_value, unescape as unescape_value};

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF16_LE_BOM: &[u8] = &[0xFF, 0xFE];
const UTF16_BE_BOM: &[u8] = &[0xFE, 0xFF];
const USER_DATA_TEXT_LOAD_LIMIT_BYTES: u64 = 1024 * 1024;
const CP_KOREAN: u32 = 949;
const CP_EUC_KR: u32 = 51949;
const CP_SHIFT_JIS: u32 = 932;
const CP_GB18030: u32 = 54936;
const CP_BIG5: u32 = 950;
const CP_WINDOWS_1250: u32 = 1250;
const CP_WINDOWS_1251: u32 = 1251;
const CP_WINDOWS_1252: u32 = 1252;
const CP_WINDOWS_1253: u32 = 1253;
const CP_WINDOWS_1254: u32 = 1254;
const CP_WINDOWS_1255: u32 = 1255;
const CP_WINDOWS_1256: u32 = 1256;
const CP_WINDOWS_1257: u32 = 1257;
const CP_WINDOWS_874: u32 = 874;
const LEGACY_AUTO_DETECT_ENCODINGS: [TextEncoding; 5] = [
    TextEncoding::EucKr,
    TextEncoding::Cp949,
    TextEncoding::ShiftJis,
    TextEncoding::Gb18030,
    TextEncoding::Big5,
];
const LEGACY_DETECTION_SAMPLE_CHUNK_BYTES: usize = 8 * 1024;
const LEGACY_DETECTION_SAMPLE_SEARCH_BYTES: usize = 64 * 1024;
const LEGACY_ROUND_TRIP_BYTE_CHUNK_BYTES: usize = 64 * 1024;
const BOMLESS_UTF16_DETECTION_SAMPLE_CHUNK_BYTES: usize = 16 * 1024;
const BOMLESS_UTF16_DETECTION_FULL_SCAN_BYTES: usize =
    BOMLESS_UTF16_DETECTION_SAMPLE_CHUNK_BYTES * 3;
const BOMLESS_UTF16_ZERO_UNIT_PERCENT: usize = 30;
const ROUND_TRIP_COMPARE_TEXT_CHUNK_BYTES: usize = 64 * 1024;
static SAVE_TOKEN_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn save_token() -> String {
    let sequence = SAVE_TOKEN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let time_marker = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("{}", duration.as_millis()),
        Err(error) => format!("pre-epoch-{}", error.duration().as_millis()),
    };
    format!("{}-{time_marker}-{sequence}", std::process::id())
}

mod atomic_writer {
    use super::*;

    pub(super) fn write_text(path: &Path, text: &str) -> Result<(), AppError> {
        write_bytes(path, text.as_bytes())
    }

    pub(super) fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
        write_atomic(
            path,
            SaveTargetExpectation::Any,
            InitialTargetCheck::Required,
            |writer, temp_path| write_all_temp(writer, bytes, temp_path),
            |_| Ok(()),
        )
        .map(|_| ())
    }

    pub(super) fn write_encoded_text(
        path: &Path,
        text: &str,
        encoding: TextEncoding,
        line_ending: LineEnding,
        target_expectation: SaveTargetExpectation,
        initial_target_check: InitialTargetCheck,
    ) -> Result<SavedContentFingerprint, AppError> {
        write_encoded_text_with_limit(
            path,
            text,
            encoding,
            line_ending,
            target_expectation,
            initial_target_check,
            MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES,
        )
    }

    pub(super) fn write_encoded_text_with_limit(
        path: &Path,
        text: &str,
        encoding: TextEncoding,
        line_ending: LineEnding,
        target_expectation: SaveTargetExpectation,
        initial_target_check: InitialTargetCheck,
        saved_document_byte_limit: u64,
    ) -> Result<SavedContentFingerprint, AppError> {
        let (written, metadata) = write_atomic(
            path,
            target_expectation,
            initial_target_check,
            |writer, temp_path| {
                let mut writer = ContentFingerprintWriter::new(writer, saved_document_byte_limit);
                text_codec::write_encoded_normalized(
                    &mut writer,
                    text,
                    encoding,
                    line_ending,
                    temp_path,
                )?;
                Ok(writer.finish())
            },
            |written| {
                validate_saved_document_byte_len_with_limit(
                    path,
                    written.byte_len,
                    saved_document_byte_limit,
                )
            },
        )?;
        Ok(SavedContentFingerprint { written, metadata })
    }

    fn write_atomic<F, V, T>(
        path: &Path,
        target_expectation: SaveTargetExpectation,
        initial_target_check: InitialTargetCheck,
        write_contents: F,
        validate_written: V,
    ) -> Result<(T, FileMetadataSnapshot), AppError>
    where
        F: FnOnce(&mut BufWriter<File>, &Path) -> Result<T, AppError>,
        V: FnOnce(&T) -> Result<(), AppError>,
    {
        let token = save_token();
        let plan = SavePolicy::AtomicReplace
            .plan(path, &token)
            .map_err(|error| save_policy_error(error, path))?;
        let temp_path = plan.temp_path.clone();
        let user_target_path = path.to_path_buf();

        // Keep the target sharing guard alive through the atomic replace.
        let _target_guard = open_target_save_guard(path, target_expectation)?;
        if initial_target_check == InitialTargetCheck::Required {
            ensure_target_expectation_matches(path, target_expectation)?;
        }

        let file = File::create(&temp_path).map_err(|source| {
            AppError::io_path_with_user(
                source,
                "create temporary save file",
                temp_path.clone(),
                "save file",
                user_target_path.clone(),
            )
        })?;
        let mut writer = BufWriter::new(file);
        let written = match write_contents(&mut writer, &temp_path) {
            Ok(written) => written,
            Err(error) => {
                let error = error.with_user_io_context("save file", user_target_path.clone());
                drop(writer);
                remove_temp_file(&temp_path);
                return Err(error);
            }
        };
        if let Err(error) = validate_written(&written) {
            drop(writer);
            remove_temp_file(&temp_path);
            return Err(error);
        }
        if let Err(source) = writer.flush() {
            drop(writer);
            remove_temp_file(&temp_path);
            return Err(AppError::io_path_with_user(
                source,
                "flush temporary save file",
                temp_path,
                "save file",
                user_target_path.clone(),
            ));
        }
        let file = match writer.into_inner() {
            Ok(file) => file,
            Err(error) => {
                let source = error.into_error();
                remove_temp_file(&temp_path);
                return Err(AppError::io_path_with_user(
                    source,
                    "finish temporary save file",
                    temp_path,
                    "save file",
                    user_target_path.clone(),
                ));
            }
        };
        if let Err(source) = file.sync_all() {
            drop(file);
            remove_temp_file(&temp_path);
            return Err(AppError::io_path_with_user(
                source,
                "sync temporary save file",
                temp_path,
                "save file",
                user_target_path.clone(),
            ));
        }
        let written_metadata = match file.metadata() {
            Ok(metadata) => FileMetadataSnapshot::from_file_metadata(&file, &metadata),
            Err(source) => {
                drop(file);
                remove_temp_file(&temp_path);
                return Err(AppError::io_path_with_user(
                    source,
                    "read temporary save metadata",
                    temp_path,
                    "save file",
                    user_target_path.clone(),
                ));
            }
        };
        drop(file);

        if let Err(error) = ensure_target_expectation_matches(path, target_expectation) {
            remove_temp_file(&temp_path);
            return Err(error);
        }

        match target_expectation {
            SaveTargetExpectation::Missing => {
                replace_missing_file_atomically(&temp_path, &plan.target_path)
                    .map_err(|error| {
                        error.with_user_io_context("save file", user_target_path.clone())
                    })
                    .inspect_err(|_| remove_temp_file(&temp_path))
            }
            SaveTargetExpectation::Any
            | SaveTargetExpectation::Unchanged(_)
            | SaveTargetExpectation::UnchangedWithMetadata { .. }
            | SaveTargetExpectation::UnchangedMetadata(_) => platform::replace_file_atomically(
                &temp_path,
                &plan.target_path,
                target_expectation.target_must_exist(),
            )
            .map_err(|error| error.with_user_io_context("save file", user_target_path))
            .inspect_err(|_| remove_temp_file(&temp_path)),
        }?;
        Ok((written, written_metadata))
    }

    fn save_policy_error(error: SavePolicyError, path: &Path) -> AppError {
        let message = match error {
            SavePolicyError::MissingFileName => "target path has no file name",
            SavePolicyError::EmptyToken => "temporary save token is empty",
        };
        AppError::file_access(
            FileAccessKind::Other,
            io::Error::new(ErrorKind::InvalidInput, message),
            "prepare save policy",
            Some(path.to_path_buf()),
        )
    }

    fn replace_missing_file_atomically(
        temp_path: &Path,
        target_path: &Path,
    ) -> Result<(), AppError> {
        fs::rename(temp_path, target_path).map_err(|source| {
            if target_path.try_exists().unwrap_or(false) {
                AppError::external_file_changed(target_path.to_path_buf())
            } else {
                AppError::io_path(source, "replace saved file", target_path.to_path_buf())
            }
        })
    }

    fn remove_temp_file(path: &Path) {
        let _ = fs::remove_file(path);
    }
}

const WRITE_CHUNK_BYTES: usize = 64 * 1024;

fn write_all_temp<W: Write + ?Sized>(
    writer: &mut W,
    bytes: &[u8],
    temp_path: &Path,
) -> Result<(), AppError> {
    writer.write_all(bytes).map_err(|source| {
        AppError::io_path(source, "write temporary save file", temp_path.to_path_buf())
    })
}

struct ContentFingerprintWriter<'a, W: Write + ?Sized> {
    inner: &'a mut W,
    content_fingerprint: u64,
    byte_len: u64,
    byte_limit: u64,
    limit_exceeded: bool,
}

impl<'a, W: Write + ?Sized> ContentFingerprintWriter<'a, W> {
    fn new(inner: &'a mut W, byte_limit: u64) -> Self {
        Self {
            inner,
            content_fingerprint: CONTENT_FINGERPRINT_OFFSET,
            byte_len: 0,
            byte_limit,
            limit_exceeded: false,
        }
    }

    fn finish(self) -> WrittenContentFingerprint {
        WrittenContentFingerprint {
            content_fingerprint: self.content_fingerprint,
            byte_len: self.byte_len,
        }
    }
}

impl<W: Write + ?Sized> Write for ContentFingerprintWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let added = u64::try_from(buf.len()).map_err(|_| {
            io::Error::new(
                ErrorKind::InvalidData,
                "saved document write buffer length does not fit in the size check",
            )
        })?;
        let next_byte_len = self.byte_len.checked_add(added).ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                "saved document byte length does not fit in the size check",
            )
        })?;
        let should_write = !self.limit_exceeded && next_byte_len <= self.byte_limit;

        self.content_fingerprint = update_content_fingerprint(self.content_fingerprint, buf);
        self.byte_len = next_byte_len;

        // Keep walking the encoded stream for the exact byte count, but stop growing
        // the doomed temp file once the save limit has been crossed.
        if should_write {
            self.inner.write_all(buf)?;
        } else {
            self.limit_exceeded = true;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

mod text_codec {
    use super::*;

    pub(super) fn decode_document_bytes(
        bytes: Vec<u8>,
        requested_encoding: Option<TextEncoding>,
    ) -> Result<(String, TextEncoding), AppError> {
        decode_bytes_owned(bytes, requested_encoding, NulTextPolicy::ALLOW)
    }

    pub(super) fn ensure_encodable(content: &str, encoding: TextEncoding) -> Result<(), AppError> {
        if encoding.can_encode_all_unicode() {
            return Ok(());
        }
        match encoding {
            TextEncoding::Iso88591 => ensure_iso_8859_1_encodable(content),
            encoding => ensure_code_page_encodable(content, encoding),
        }
    }

    pub(super) fn write_encoded_normalized<W: Write + ?Sized>(
        writer: &mut W,
        text: &str,
        encoding: TextEncoding,
        line_ending: LineEnding,
        temp_path: &Path,
    ) -> Result<(), AppError> {
        super::write_encoded_normalized(writer, text, encoding, line_ending, temp_path)
    }

    fn ensure_iso_8859_1_encodable(content: &str) -> Result<(), AppError> {
        for ch in content.chars() {
            if ch as u32 > 0xff {
                return Err(AppError::encoding_encode(
                    "Text contains characters that cannot be saved as ISO-8859-1",
                ));
            }
        }
        Ok(())
    }

    fn ensure_code_page_encodable(content: &str, encoding: TextEncoding) -> Result<(), AppError> {
        let (code_page, label) = code_page_encoding(encoding)
            .ok_or(AppError::InvalidState("Unsupported code page encoding"))?;
        let mut chunk = String::with_capacity(WRITE_CHUNK_BYTES);

        for ch in content.chars() {
            chunk.push(ch);
            if chunk.len() >= WRITE_CHUNK_BYTES {
                encode_with_code_page(&chunk, code_page, label).map(|_| ())?;
                chunk.clear();
            }
        }

        if chunk.is_empty() {
            return Ok(());
        }
        encode_with_code_page(&chunk, code_page, label).map(|_| ())
    }
}

fn validate_saved_document_byte_len_with_limit(
    path: &Path,
    byte_len: u64,
    limit: u64,
) -> Result<(), AppError> {
    if byte_len > limit {
        return Err(AppError::file_too_large(
            path.to_path_buf(),
            byte_len,
            limit,
        ));
    }
    Ok(())
}

fn flush_encoded_buffer<W: Write + ?Sized>(
    writer: &mut W,
    buffer: &mut Vec<u8>,
    temp_path: &Path,
) -> Result<(), AppError> {
    if buffer.is_empty() {
        return Ok(());
    }
    write_all_temp(writer, buffer, temp_path)?;
    buffer.clear();
    Ok(())
}

fn write_encoded_normalized<W: Write + ?Sized>(
    writer: &mut W,
    text: &str,
    encoding: TextEncoding,
    line_ending: LineEnding,
    temp_path: &Path,
) -> Result<(), AppError> {
    match encoding {
        TextEncoding::Utf8 => write_utf8_normalized(writer, text, line_ending, temp_path),
        TextEncoding::Utf8Bom => {
            write_all_temp(writer, UTF8_BOM, temp_path)?;
            write_utf8_normalized(writer, text, line_ending, temp_path)
        }
        TextEncoding::Utf16Le => {
            write_all_temp(writer, UTF16_LE_BOM, temp_path)?;
            write_utf16_normalized(writer, text, line_ending, true, temp_path)
        }
        TextEncoding::Utf16Be => {
            write_all_temp(writer, UTF16_BE_BOM, temp_path)?;
            write_utf16_normalized(writer, text, line_ending, false, temp_path)
        }
        TextEncoding::Iso88591 => write_iso_8859_1_normalized(writer, text, line_ending, temp_path),
        encoding => write_code_page_normalized(writer, text, encoding, line_ending, temp_path),
    }
}

fn write_utf8_normalized<W: Write + ?Sized>(
    writer: &mut W,
    text: &str,
    line_ending: LineEnding,
    temp_path: &Path,
) -> Result<(), AppError> {
    let normalized_prefix_len = line_ending.normalized_prefix_len(text);
    let (normalized_prefix, unnormalized_suffix) = text.split_at(normalized_prefix_len);

    for chunk in normalized_prefix.as_bytes().chunks(WRITE_CHUNK_BYTES) {
        write_all_temp(writer, chunk, temp_path)?;
    }

    if unnormalized_suffix.is_empty() {
        return Ok(());
    }

    let mut buffer = Vec::with_capacity(WRITE_CHUNK_BYTES);
    line_ending.try_for_each_normalized_char(unnormalized_suffix, |ch| {
        let mut encoded = [0u8; 4];
        buffer.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
        if buffer.len() >= WRITE_CHUNK_BYTES {
            flush_encoded_buffer(writer, &mut buffer, temp_path)?;
        }
        Ok(())
    })?;
    flush_encoded_buffer(writer, &mut buffer, temp_path)
}

fn write_utf16_normalized<W: Write + ?Sized>(
    writer: &mut W,
    text: &str,
    line_ending: LineEnding,
    little_endian: bool,
    temp_path: &Path,
) -> Result<(), AppError> {
    let mut buffer = Vec::with_capacity(WRITE_CHUNK_BYTES);
    line_ending.try_for_each_normalized_char(text, |ch| {
        let mut units = [0u16; 2];
        for unit in ch.encode_utf16(&mut units).iter().copied() {
            let raw = if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            };
            buffer.extend_from_slice(&raw);
        }
        if buffer.len() >= WRITE_CHUNK_BYTES {
            flush_encoded_buffer(writer, &mut buffer, temp_path)?;
        }
        Ok(())
    })?;
    flush_encoded_buffer(writer, &mut buffer, temp_path)
}

fn write_iso_8859_1_normalized<W: Write + ?Sized>(
    writer: &mut W,
    text: &str,
    line_ending: LineEnding,
    temp_path: &Path,
) -> Result<(), AppError> {
    let mut buffer = Vec::with_capacity(WRITE_CHUNK_BYTES);
    line_ending.try_for_each_normalized_char(text, |ch| {
        let code = ch as u32;
        if code > 0xff {
            return Err(AppError::encoding_encode(
                "Text contains characters that cannot be saved as ISO-8859-1",
            ));
        }
        buffer.push(code as u8);
        if buffer.len() >= WRITE_CHUNK_BYTES {
            flush_encoded_buffer(writer, &mut buffer, temp_path)?;
        }
        Ok(())
    })?;
    flush_encoded_buffer(writer, &mut buffer, temp_path)
}

fn write_code_page_normalized<W: Write + ?Sized>(
    writer: &mut W,
    text: &str,
    encoding: TextEncoding,
    line_ending: LineEnding,
    temp_path: &Path,
) -> Result<(), AppError> {
    let mut chunk = String::with_capacity(WRITE_CHUNK_BYTES);
    line_ending.try_for_each_normalized_char(text, |ch| {
        chunk.push(ch);
        if chunk.len() >= WRITE_CHUNK_BYTES {
            flush_encoded_text_chunk(writer, &mut chunk, encoding, temp_path)?;
        }
        Ok(())
    })?;
    flush_encoded_text_chunk(writer, &mut chunk, encoding, temp_path)
}

fn flush_encoded_text_chunk<W: Write + ?Sized>(
    writer: &mut W,
    chunk: &mut String,
    encoding: TextEncoding,
    temp_path: &Path,
) -> Result<(), AppError> {
    if chunk.is_empty() {
        return Ok(());
    }
    let bytes = encode_text(chunk, encoding)?;
    write_all_temp(writer, &bytes, temp_path)?;
    chunk.clear();
    Ok(())
}

#[derive(Clone, Copy)]
struct NulTextPolicy {
    reject: bool,
}

impl NulTextPolicy {
    const ALLOW: Self = Self { reject: false };
    #[cfg(test)]
    const REJECT: Self = Self { reject: true };

    fn apply(self, content: String) -> Result<String, AppError> {
        if self.reject {
            reject_nul_text(content)
        } else {
            Ok(content)
        }
    }
}

fn decode_bytes_owned(
    mut bytes: Vec<u8>,
    requested_encoding: Option<TextEncoding>,
    nul_policy: NulTextPolicy,
) -> Result<(String, TextEncoding), AppError> {
    if bytes.is_empty() {
        return Ok((
            String::new(),
            requested_encoding.unwrap_or(TextEncoding::Utf8),
        ));
    }

    if let Some(encoding) = requested_encoding {
        return match encoding {
            TextEncoding::Utf8 => decode_utf8_owned(bytes, "Invalid UTF-8 input", nul_policy)
                .map(|content| (content, encoding)),
            TextEncoding::Utf8Bom => {
                if bytes.starts_with(UTF8_BOM) {
                    bytes.drain(..UTF8_BOM.len());
                }
                decode_utf8_owned(bytes, "Invalid UTF-8 BOM input", nul_policy)
                    .map(|content| (content, encoding))
            }
            _ => decode_with_encoding(&bytes, encoding, nul_policy)
                .map(|content| (content, encoding)),
        };
    }

    if bytes.starts_with(UTF8_BOM) {
        bytes.drain(..UTF8_BOM.len());
        return decode_utf8_owned(bytes, "Invalid UTF-8 BOM input", nul_policy)
            .map(|content| (content, TextEncoding::Utf8Bom));
    }

    if bytes.starts_with(UTF16_LE_BOM) {
        return decode_with_encoding(&bytes, TextEncoding::Utf16Le, nul_policy)
            .map(|content| (content, TextEncoding::Utf16Le));
    }

    if bytes.starts_with(UTF16_BE_BOM) {
        return decode_with_encoding(&bytes, TextEncoding::Utf16Be, nul_policy)
            .map(|content| (content, TextEncoding::Utf16Be));
    }

    if let Some((content, encoding)) = detect_bomless_utf16(&bytes, nul_policy) {
        return Ok((content, encoding));
    }

    let bytes = match String::from_utf8(bytes) {
        Ok(content) => {
            return nul_policy
                .apply(content)
                .map(|content| (content, TextEncoding::Utf8));
        }
        Err(error) => error.into_bytes(),
    };

    let legacy_sample_ranges = LegacySampleRanges::for_bytes(&bytes);
    let needs_full_round_trip = legacy_sample_ranges.needs_full_round_trip();
    for encoding in LEGACY_AUTO_DETECT_ENCODINGS {
        if !legacy_sample_ranges.round_trip(&bytes, encoding, nul_policy) {
            continue;
        }
        if needs_full_round_trip {
            if let Some(content) = decode_legacy_with_full_round_trip(&bytes, encoding, nul_policy)
            {
                return Ok((content, encoding));
            }
            continue;
        }
        if let Ok(content) = decode_with_encoding(&bytes, encoding, nul_policy) {
            return Ok((content, encoding));
        }
    }

    decode_with_encoding(&bytes, TextEncoding::Iso88591, nul_policy)
        .map(|content| (content, TextEncoding::Iso88591))
}

fn decode_utf8_owned(
    bytes: Vec<u8>,
    message: &str,
    nul_policy: NulTextPolicy,
) -> Result<String, AppError> {
    String::from_utf8(bytes)
        .map_err(|_| AppError::encoding_decode(message))
        .and_then(|content| nul_policy.apply(content))
}

fn reject_nul_text(content: String) -> Result<String, AppError> {
    reject_nul_presence(content.contains('\0')).map(|()| content)
}

fn reject_nul_presence(contains_nul: bool) -> Result<(), AppError> {
    if contains_nul {
        return Err(AppError::encoding_unsafe_text(
            "Text contains NUL characters and cannot be opened safely",
        ));
    }
    Ok(())
}

#[cfg(test)]
fn decode_bytes(
    bytes: &[u8],
    requested_encoding: Option<TextEncoding>,
) -> Result<(String, TextEncoding), AppError> {
    decode_bytes_owned(bytes.to_vec(), requested_encoding, NulTextPolicy::REJECT)
}

fn round_trips(bytes: &[u8], content: &str, encoding: TextEncoding) -> bool {
    if matches!(
        encoding,
        TextEncoding::Utf8Bom | TextEncoding::Utf16Le | TextEncoding::Utf16Be
    ) {
        return encode_text(content, encoding).is_ok_and(|encoded| encoded == bytes);
    }

    if let Some((code_page, label)) = code_page_encoding(encoding) {
        return round_trips_code_page(bytes, content, code_page, label);
    }

    let mut text_offset = 0usize;
    let mut byte_offset = 0usize;

    while text_offset < content.len() {
        let mut text_end = text_offset
            .saturating_add(ROUND_TRIP_COMPARE_TEXT_CHUNK_BYTES)
            .min(content.len());
        while !content.is_char_boundary(text_end) {
            text_end -= 1;
        }
        if text_end == text_offset {
            return false;
        }

        let Ok(encoded) = encode_text(&content[text_offset..text_end], encoding) else {
            return false;
        };
        let Some(byte_end) = byte_offset.checked_add(encoded.len()) else {
            return false;
        };
        if byte_end > bytes.len() || &bytes[byte_offset..byte_end] != encoded.as_slice() {
            return false;
        }

        text_offset = text_end;
        byte_offset = byte_end;
    }

    byte_offset == bytes.len()
}

fn round_trips_code_page(bytes: &[u8], content: &str, code_page: u32, label: &str) -> bool {
    let mut buffer = platform::CodePageEncodeBuffer::default();
    round_trips_code_page_reusing(bytes, content, code_page, label, &mut buffer)
}

fn round_trips_code_page_reusing(
    bytes: &[u8],
    content: &str,
    code_page: u32,
    label: &str,
    buffer: &mut platform::CodePageEncodeBuffer,
) -> bool {
    let mut text_offset = 0usize;
    let mut byte_offset = 0usize;

    while text_offset < content.len() {
        let mut text_end = text_offset
            .saturating_add(ROUND_TRIP_COMPARE_TEXT_CHUNK_BYTES)
            .min(content.len());
        while !content.is_char_boundary(text_end) {
            text_end -= 1;
        }
        if text_end == text_offset {
            return false;
        }

        let Ok(encoded) = platform::encode_code_page_reusing(
            &content[text_offset..text_end],
            code_page,
            label,
            buffer,
        ) else {
            return false;
        };
        let Some(byte_end) = byte_offset.checked_add(encoded.len()) else {
            return false;
        };
        if byte_end > bytes.len() || &bytes[byte_offset..byte_end] != encoded {
            return false;
        }

        text_offset = text_end;
        byte_offset = byte_end;
    }

    byte_offset == bytes.len()
}

fn decode_legacy_with_full_round_trip(
    bytes: &[u8],
    encoding: TextEncoding,
    nul_policy: NulTextPolicy,
) -> Option<String> {
    let Some((code_page, label)) = code_page_encoding(encoding) else {
        let content = decode_with_encoding(bytes, encoding, nul_policy).ok()?;
        return round_trips(bytes, &content, encoding).then_some(content);
    };
    decode_code_page_with_full_round_trip(bytes, code_page, label, nul_policy)
}

fn decode_code_page_with_full_round_trip(
    bytes: &[u8],
    code_page: u32,
    label: &str,
    nul_policy: NulTextPolicy,
) -> Option<String> {
    let mut content = String::new();
    let mut byte_offset = 0usize;
    let mut buffer = platform::CodePageEncodeBuffer::default();

    while byte_offset < bytes.len() {
        let byte_end = legacy_round_trip_byte_chunk_end(bytes, byte_offset);
        if byte_end <= byte_offset {
            return None;
        }

        let chunk = &bytes[byte_offset..byte_end];
        let decoded = decode_with_code_page(chunk, code_page, label).ok()?;
        let decoded = nul_policy.apply(decoded).ok()?;
        if !round_trips_code_page_reusing(chunk, &decoded, code_page, label, &mut buffer) {
            return None;
        }

        if byte_offset == 0 && byte_end == bytes.len() {
            return Some(decoded);
        }
        if byte_offset == 0 {
            content = decoded;
            content.reserve(bytes.len().saturating_sub(content.len()));
        } else {
            content.push_str(&decoded);
        }
        byte_offset = byte_end;
    }

    Some(content)
}

fn legacy_round_trip_byte_chunk_end(bytes: &[u8], offset: usize) -> usize {
    let target = offset
        .saturating_add(LEGACY_ROUND_TRIP_BYTE_CHUNK_BYTES)
        .min(bytes.len());
    if target == bytes.len() {
        return target;
    }

    if let Some(relative_end) = find_last_line_break(&bytes[offset..target]) {
        return offset + relative_end + 1;
    }

    find_line_break(&bytes[target..])
        .map(|relative_end| target + relative_end + 1)
        .unwrap_or(bytes.len())
}

#[derive(Clone, Copy)]
struct LegacySampleRanges {
    ranges: [(usize, usize); 3],
    range_count: usize,
    required: bool,
}

impl LegacySampleRanges {
    fn for_bytes(bytes: &[u8]) -> Self {
        if bytes.len() <= LEGACY_DETECTION_SAMPLE_CHUNK_BYTES {
            return Self {
                ranges: [(0usize, 0usize); 3],
                range_count: 0,
                required: false,
            };
        }

        let mut ranges = [(0usize, 0usize); 3];
        let mut range_count = 0usize;
        push_legacy_sample_range(
            bytes,
            &mut ranges,
            &mut range_count,
            prefix_legacy_sample_range(bytes),
        );
        push_legacy_sample_range(
            bytes,
            &mut ranges,
            &mut range_count,
            middle_legacy_sample_range(bytes),
        );
        push_legacy_sample_range(
            bytes,
            &mut ranges,
            &mut range_count,
            suffix_legacy_sample_range(bytes),
        );

        Self {
            ranges,
            range_count,
            required: true,
        }
    }

    fn round_trip(self, bytes: &[u8], encoding: TextEncoding, nul_policy: NulTextPolicy) -> bool {
        if !self.required {
            return true;
        }

        if let Some((code_page, label)) = code_page_encoding(encoding) {
            let mut buffer = platform::CodePageEncodeBuffer::default();
            return self.ranges[..self.range_count].iter().all(|&(start, end)| {
                let sample = &bytes[start..end];
                let Ok(content) = decode_with_code_page(sample, code_page, label)
                    .and_then(|content| nul_policy.apply(content))
                else {
                    return false;
                };
                round_trips_code_page_reusing(sample, &content, code_page, label, &mut buffer)
            });
        }

        self.ranges[..self.range_count].iter().all(|&(start, end)| {
            let sample = &bytes[start..end];
            decode_with_encoding(sample, encoding, nul_policy)
                .is_ok_and(|content| round_trips(sample, &content, encoding))
        })
    }

    fn needs_full_round_trip(self) -> bool {
        !self.required || self.range_count < self.ranges.len()
    }
}

#[cfg(test)]
fn legacy_samples_round_trip(bytes: &[u8], encoding: TextEncoding) -> bool {
    LegacySampleRanges::for_bytes(bytes).round_trip(bytes, encoding, NulTextPolicy::REJECT)
}

#[cfg(test)]
fn legacy_samples_need_full_round_trip(bytes: &[u8]) -> bool {
    LegacySampleRanges::for_bytes(bytes).needs_full_round_trip()
}

fn push_legacy_sample_range(
    bytes: &[u8],
    ranges: &mut [(usize, usize); 3],
    range_count: &mut usize,
    range: Option<(usize, usize)>,
) {
    let Some((start, end)) = range else {
        return;
    };
    if start >= end || end > bytes.len() || *range_count >= ranges.len() {
        return;
    }
    if ranges[..*range_count]
        .iter()
        .any(|&(existing_start, existing_end)| existing_start == start && existing_end == end)
    {
        return;
    }

    ranges[*range_count] = (start, end);
    *range_count += 1;
}

// CR/LF bytes are single-byte line endings in every legacy auto-detect code page.
fn prefix_legacy_sample_range(bytes: &[u8]) -> Option<(usize, usize)> {
    let end_limit = LEGACY_DETECTION_SAMPLE_CHUNK_BYTES.min(bytes.len());
    find_last_line_break(&bytes[..end_limit]).map(|end| (0, end + 1))
}

fn middle_legacy_sample_range(bytes: &[u8]) -> Option<(usize, usize)> {
    let anchor = bytes.len() / 2;
    let search_start = anchor.saturating_sub(LEGACY_DETECTION_SAMPLE_SEARCH_BYTES);
    let start = find_last_line_break(&bytes[search_start..anchor])
        .map(|relative_start| search_start + relative_start + 1)?;
    if start >= bytes.len() {
        return None;
    }

    let end_limit = (start + LEGACY_DETECTION_SAMPLE_CHUNK_BYTES).min(bytes.len());
    if end_limit == bytes.len() {
        return Some((start, end_limit));
    }
    find_last_line_break(&bytes[start..end_limit])
        .map(|relative_end| (start, start + relative_end + 1))
}

fn suffix_legacy_sample_range(bytes: &[u8]) -> Option<(usize, usize)> {
    let min_start = bytes
        .len()
        .saturating_sub(LEGACY_DETECTION_SAMPLE_CHUNK_BYTES);
    find_line_break(&bytes[min_start..]).and_then(|relative_start| {
        let start = min_start + relative_start + 1;
        (start < bytes.len()).then_some((start, bytes.len()))
    })
}

fn find_line_break(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(|byte| matches!(*byte, b'\n' | b'\r'))
}

fn find_last_line_break(bytes: &[u8]) -> Option<usize> {
    bytes
        .iter()
        .rposition(|byte| matches!(*byte, b'\n' | b'\r'))
}

fn detect_bomless_utf16(bytes: &[u8], nul_policy: NulTextPolicy) -> Option<(String, TextEncoding)> {
    if looks_like_utf16(bytes, true)
        && let Ok(content) = decode_with_encoding(bytes, TextEncoding::Utf16Le, nul_policy)
    {
        return Some((content, TextEncoding::Utf16Le));
    }

    if looks_like_utf16(bytes, false)
        && let Ok(content) = decode_with_encoding(bytes, TextEncoding::Utf16Be, nul_policy)
    {
        return Some((content, TextEncoding::Utf16Be));
    }

    None
}

fn looks_like_utf16(bytes: &[u8], little_endian: bool) -> bool {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return false;
    }

    let mut units = 0usize;
    let mut zeroes = 0usize;

    if bytes.len() <= BOMLESS_UTF16_DETECTION_FULL_SCAN_BYTES {
        count_utf16_zero_units(bytes, little_endian, &mut units, &mut zeroes);
    } else {
        let chunk = BOMLESS_UTF16_DETECTION_SAMPLE_CHUNK_BYTES;
        let middle_start = ((bytes.len() / 2) - (chunk / 2)) & !1;
        let suffix_start = bytes.len() - chunk;

        for (start, end) in [
            (0, chunk),
            (middle_start, middle_start + chunk),
            (suffix_start, bytes.len()),
        ] {
            count_utf16_zero_units(&bytes[start..end], little_endian, &mut units, &mut zeroes);
        }
    }

    units > 0 && zeroes * 100 / units >= BOMLESS_UTF16_ZERO_UNIT_PERCENT
}

fn count_utf16_zero_units(
    bytes: &[u8],
    little_endian: bool,
    units: &mut usize,
    zeroes: &mut usize,
) {
    *units += bytes.len() / 2;
    *zeroes += bytes
        .chunks_exact(2)
        .filter(|pair| {
            if little_endian {
                pair[1] == 0
            } else {
                pair[0] == 0
            }
        })
        .count();
}

fn code_page_encoding(encoding: TextEncoding) -> Option<(u32, &'static str)> {
    match encoding {
        TextEncoding::Cp949 => Some((CP_KOREAN, "CP949")),
        TextEncoding::EucKr => Some((CP_EUC_KR, "EUC-KR")),
        TextEncoding::ShiftJis => Some((CP_SHIFT_JIS, "Shift-JIS")),
        TextEncoding::Gb18030 => Some((CP_GB18030, "GB18030")),
        TextEncoding::Big5 => Some((CP_BIG5, "Big5")),
        TextEncoding::Windows1250 => Some((CP_WINDOWS_1250, "Windows-1250")),
        TextEncoding::Windows1251 => Some((CP_WINDOWS_1251, "Windows-1251")),
        TextEncoding::Windows1252 => Some((CP_WINDOWS_1252, "Windows-1252")),
        TextEncoding::Windows1253 => Some((CP_WINDOWS_1253, "Windows-1253")),
        TextEncoding::Windows1254 => Some((CP_WINDOWS_1254, "Windows-1254")),
        TextEncoding::Windows1255 => Some((CP_WINDOWS_1255, "Windows-1255")),
        TextEncoding::Windows1256 => Some((CP_WINDOWS_1256, "Windows-1256")),
        TextEncoding::Windows1257 => Some((CP_WINDOWS_1257, "Windows-1257")),
        TextEncoding::Windows874 => Some((CP_WINDOWS_874, "Windows-874")),
        _ => None,
    }
}

fn decode_with_encoding(
    bytes: &[u8],
    encoding: TextEncoding,
    nul_policy: NulTextPolicy,
) -> Result<String, AppError> {
    let content = match encoding {
        TextEncoding::Utf8 => std::str::from_utf8(bytes)
            .map(ToOwned::to_owned)
            .map_err(|_| AppError::encoding_decode("Invalid UTF-8 input")),
        TextEncoding::Utf8Bom => {
            let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
            std::str::from_utf8(bytes)
                .map(ToOwned::to_owned)
                .map_err(|_| AppError::encoding_decode("Invalid UTF-8 BOM input"))
        }
        TextEncoding::Utf16Le => decode_utf16(bytes, true),
        TextEncoding::Utf16Be => decode_utf16(bytes, false),
        TextEncoding::Iso88591 => Ok(bytes.iter().map(|byte| char::from(*byte)).collect()),
        encoding => {
            let (code_page, label) = code_page_encoding(encoding)
                .ok_or(AppError::InvalidState("Unsupported code page encoding"))?;
            decode_with_code_page(bytes, code_page, label)
        }
    }?;
    nul_policy.apply(content)
}

fn encode_text(text: &str, encoding: TextEncoding) -> Result<Vec<u8>, AppError> {
    match encoding {
        TextEncoding::Utf8 => Ok(text.as_bytes().to_vec()),
        TextEncoding::Utf8Bom => {
            let mut bytes = Vec::with_capacity(UTF8_BOM.len() + text.len());
            bytes.extend_from_slice(UTF8_BOM);
            bytes.extend_from_slice(text.as_bytes());
            Ok(bytes)
        }
        TextEncoding::Utf16Le => encode_utf16(text, true),
        TextEncoding::Utf16Be => encode_utf16(text, false),
        TextEncoding::Iso88591 => encode_iso_8859_1(text),
        encoding => {
            let (code_page, label) = code_page_encoding(encoding)
                .ok_or(AppError::InvalidState("Unsupported code page encoding"))?;
            encode_with_code_page(text, code_page, label)
        }
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, AppError> {
    let bytes = if little_endian {
        bytes.strip_prefix(UTF16_LE_BOM).unwrap_or(bytes)
    } else {
        bytes.strip_prefix(UTF16_BE_BOM).unwrap_or(bytes)
    };
    if bytes.len() % 2 != 0 {
        return Err(AppError::encoding_decode(
            "UTF-16 input has an odd byte length",
        ));
    }

    let units = bytes.chunks_exact(2).map(|pair| {
        if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        }
    });
    let mut content = String::with_capacity(bytes.len() / 2);
    for decoded in std::char::decode_utf16(units) {
        content.push(decoded.map_err(|_| AppError::encoding_decode("Invalid UTF-16 input"))?);
    }

    Ok(content)
}

fn encode_utf16(text: &str, little_endian: bool) -> Result<Vec<u8>, AppError> {
    let mut bytes = Vec::with_capacity(2 + text.len() * 2);
    if little_endian {
        bytes.extend_from_slice(UTF16_LE_BOM);
    } else {
        bytes.extend_from_slice(UTF16_BE_BOM);
    }

    for unit in text.encode_utf16() {
        let raw = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        bytes.extend_from_slice(&raw);
    }

    Ok(bytes)
}

fn encode_iso_8859_1(text: &str) -> Result<Vec<u8>, AppError> {
    let mut bytes = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        if code > 0xff {
            return Err(AppError::encoding_encode(
                "Text contains characters that cannot be saved as ISO-8859-1",
            ));
        }
        bytes.push(code as u8);
    }
    Ok(bytes)
}

fn decode_with_code_page(bytes: &[u8], code_page: u32, label: &str) -> Result<String, AppError> {
    platform::decode_code_page(bytes, code_page, label)
}

fn encode_with_code_page(text: &str, code_page: u32, label: &str) -> Result<Vec<u8>, AppError> {
    platform::encode_code_page(text, code_page, label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::EditorApp;
    use crate::domain::{Document, DocumentId};
    use crate::error::EncodingErrorKind;
    use std::collections::HashSet;
    #[cfg(windows)]
    use std::ffi::OsString;
    #[cfg(windows)]
    use std::fs::OpenOptions;
    #[cfg(windows)]
    use std::os::windows::ffi::OsStringExt;
    #[cfg(windows)]
    use std::os::windows::fs::OpenOptionsExt;

    #[derive(Clone, Copy)]
    struct EncodedSample {
        name: &'static str,
        bytes: &'static [u8],
        text: &'static str,
        encoding: TextEncoding,
        line_ending: LineEnding,
    }

    const UTF8_CRLF_SAMPLE: EncodedSample = EncodedSample {
        name: "utf-8 crlf",
        bytes: b"alpha\r\nbeta",
        text: "alpha\r\nbeta",
        encoding: TextEncoding::Utf8,
        line_ending: LineEnding::Crlf,
    };
    const UTF8_BOM_LF_SAMPLE: EncodedSample = EncodedSample {
        name: "utf-8 bom lf",
        bytes: b"\xEF\xBB\xBFalpha\nbeta",
        text: "alpha\nbeta",
        encoding: TextEncoding::Utf8Bom,
        line_ending: LineEnding::Lf,
    };
    const UTF16_LE_CR_SAMPLE: EncodedSample = EncodedSample {
        name: "utf-16 le cr",
        bytes: b"\xFF\xFEA\x00\r\x00B\x00",
        text: "A\rB",
        encoding: TextEncoding::Utf16Le,
        line_ending: LineEnding::Cr,
    };
    const UTF16_BE_CRLF_SAMPLE: EncodedSample = EncodedSample {
        name: "utf-16 be crlf",
        bytes: b"\xFE\xFF\x00A\x00\r\x00\n\x00B",
        text: "A\r\nB",
        encoding: TextEncoding::Utf16Be,
        line_ending: LineEnding::Crlf,
    };
    const EUC_KR_LF_SAMPLE: EncodedSample = EncodedSample {
        name: "euc-kr lf",
        bytes: b"\xC7\xD1\xB1\xDB\n",
        text: "한글\n",
        encoding: TextEncoding::EucKr,
        line_ending: LineEnding::Lf,
    };
    const CP949_CRLF_SAMPLE: EncodedSample = EncodedSample {
        name: "cp949 crlf",
        bytes: b"\x94\xEE\r\n",
        text: "뷁\r\n",
        encoding: TextEncoding::Cp949,
        line_ending: LineEnding::Crlf,
    };
    const SHIFT_JIS_CR_SAMPLE: EncodedSample = EncodedSample {
        name: "shift-jis cr",
        bytes: b"\x93\xFA\x96\x7B\x8C\xEA\r",
        text: "日本語\r",
        encoding: TextEncoding::ShiftJis,
        line_ending: LineEnding::Cr,
    };
    const ISO_8859_1_LF_SAMPLE: EncodedSample = EncodedSample {
        name: "iso-8859-1 lf",
        bytes: b"caf\xE9\n",
        text: "café\n",
        encoding: TextEncoding::Iso88591,
        line_ending: LineEnding::Lf,
    };

    const ENCODED_SAMPLES: &[EncodedSample] = &[
        UTF8_CRLF_SAMPLE,
        UTF8_BOM_LF_SAMPLE,
        UTF16_LE_CR_SAMPLE,
        UTF16_BE_CRLF_SAMPLE,
        EUC_KR_LF_SAMPLE,
        CP949_CRLF_SAMPLE,
        SHIFT_JIS_CR_SAMPLE,
        ISO_8859_1_LF_SAMPLE,
    ];

    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn new(prefix: &str) -> Self {
            let path = env::temp_dir().join(format!("{prefix}-{}", save_token()));
            fs::create_dir_all(&path).expect("create temp root");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    #[cfg(windows)]
    fn windows_path_with_unpaired_surrogate(stem: &str) -> PathBuf {
        let mut units: Vec<u16> = format!("C:\\Temp\\{stem}").encode_utf16().collect();
        units.push(0xD800);
        units.extend(".txt".encode_utf16());
        PathBuf::from(OsString::from_wide(&units))
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            clear_read_only_tree(&self.path);
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn clear_read_only_tree(path: &Path) {
        let Ok(metadata) = fs::symlink_metadata(path) else {
            return;
        };
        if metadata.is_dir()
            && let Ok(entries) = fs::read_dir(path)
        {
            for entry in entries.flatten() {
                clear_read_only_tree(&entry.path());
            }
        }
        if metadata.permissions().readonly() {
            let _ = crate::platform::clear_readonly_attribute(path);
        }
    }

    fn assert_no_temp_save_files(path: &Path) {
        let entries = fs::read_dir(path).expect("read temp root");
        let temp_files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("j3tmp"))
            .collect();
        assert!(temp_files.is_empty(), "leftover temp files: {temp_files:?}");
    }

    fn assert_file_too_large(error: AppError, expected_byte_len: u64, expected_limit: u64) {
        match error {
            AppError::FileTooLarge {
                byte_len, limit, ..
            } => {
                assert_eq!(byte_len, expected_byte_len);
                assert_eq!(limit, expected_limit);
            }
            _ => panic!("expected file-too-large error"),
        }
    }

    #[test]
    fn utf8_bom_round_trips() {
        let bytes = encode_text("hello", TextEncoding::Utf8Bom).expect("encode");
        assert!(bytes.starts_with(UTF8_BOM));
        let (text, encoding) = decode_bytes(&bytes, None).expect("decode");
        assert_eq!(text, "hello");
        assert_eq!(encoding, TextEncoding::Utf8Bom);
    }

    #[test]
    fn utf8_bom_is_written_only_for_explicit_bom_encoding() {
        assert_eq!(
            encode_text("hello", TextEncoding::Utf8).expect("encode"),
            b"hello"
        );
        assert_eq!(
            encode_text("hello", TextEncoding::Utf8Bom).expect("encode"),
            b"\xEF\xBB\xBFhello"
        );
    }

    #[test]
    fn utf16_be_round_trips() {
        let bytes = encode_text("한글", TextEncoding::Utf16Be).expect("encode");
        let (text, encoding) = decode_bytes(&bytes, None).expect("decode");
        assert_eq!(text, "한글");
        assert_eq!(encoding, TextEncoding::Utf16Be);
    }

    #[test]
    fn utf16_round_trips_surrogate_pairs() {
        for encoding in [TextEncoding::Utf16Le, TextEncoding::Utf16Be] {
            let bytes = encode_text("A😀B", encoding).expect("encode");
            let (text, detected_encoding) = decode_bytes(&bytes, None).expect("decode");

            assert_eq!(text, "A😀B");
            assert_eq!(detected_encoding, encoding);
        }
    }

    #[test]
    fn auto_detects_supported_byte_samples() {
        for sample in ENCODED_SAMPLES {
            let (text, encoding) = decode_bytes(sample.bytes, None).expect(sample.name);

            assert_eq!(text, sample.text, "{}", sample.name);
            assert_eq!(encoding, sample.encoding, "{}", sample.name);
            assert_eq!(
                LineEnding::detect(&text),
                sample.line_ending,
                "{}",
                sample.name
            );
        }
    }

    #[test]
    fn legacy_sample_filter_rejects_invalid_large_candidate() {
        let mut bytes = Vec::new();
        let mut expected = String::new();
        while bytes.len() <= LEGACY_DETECTION_SAMPLE_CHUNK_BYTES * 2 {
            bytes.extend_from_slice(SHIFT_JIS_CR_SAMPLE.bytes);
            expected.push_str(SHIFT_JIS_CR_SAMPLE.text);
        }

        assert!(!legacy_samples_round_trip(&bytes, TextEncoding::EucKr));
        assert!(legacy_samples_round_trip(&bytes, TextEncoding::ShiftJis));

        let (text, encoding) = decode_bytes(&bytes, None).expect("decode repeated shift-jis");
        assert_eq!(text, expected);
        assert_eq!(encoding, TextEncoding::ShiftJis);
    }

    #[test]
    fn legacy_auto_detect_preserves_priority_for_large_multi_match() {
        let mut bytes = Vec::new();
        let mut expected = String::new();
        while bytes.len() <= LEGACY_DETECTION_SAMPLE_CHUNK_BYTES * 2 {
            bytes.extend_from_slice(EUC_KR_LF_SAMPLE.bytes);
            expected.push_str(EUC_KR_LF_SAMPLE.text);
        }

        assert!(legacy_samples_round_trip(&bytes, TextEncoding::EucKr));
        assert!(legacy_samples_round_trip(&bytes, TextEncoding::Cp949));

        let (text, encoding) = decode_bytes(&bytes, None).expect("decode repeated euc-kr");
        assert_eq!(text, expected);
        assert_eq!(encoding, TextEncoding::EucKr);
    }

    #[test]
    fn legacy_full_round_trip_policy_requires_complete_samples_to_skip() {
        assert!(legacy_samples_need_full_round_trip(CP949_CRLF_SAMPLE.bytes));

        let mut sampled_bytes = Vec::new();
        while sampled_bytes.len() <= LEGACY_DETECTION_SAMPLE_CHUNK_BYTES * 2 {
            sampled_bytes.extend_from_slice(EUC_KR_LF_SAMPLE.bytes);
        }
        assert!(!legacy_samples_need_full_round_trip(&sampled_bytes));

        let unsampled_bytes = vec![0x80; LEGACY_DETECTION_SAMPLE_CHUNK_BYTES * 2];
        assert!(legacy_samples_need_full_round_trip(&unsampled_bytes));
    }

    #[test]
    fn legacy_auto_detect_preserves_priority_when_sparse_samples_need_full_round_trip() {
        fn append_euc_kr_until(bytes: &mut Vec<u8>, expected: &mut String, byte_len: usize) {
            while bytes.len() < byte_len {
                bytes.extend_from_slice(b"\xC7\xD1\xB1\xDB");
                expected.push_str("한글");
            }
        }

        let mut bytes = Vec::new();
        let mut expected = String::new();
        append_euc_kr_until(
            &mut bytes,
            &mut expected,
            LEGACY_ROUND_TRIP_BYTE_CHUNK_BYTES / 2,
        );
        bytes.push(b'\n');
        expected.push('\n');
        append_euc_kr_until(
            &mut bytes,
            &mut expected,
            LEGACY_ROUND_TRIP_BYTE_CHUNK_BYTES + LEGACY_ROUND_TRIP_BYTE_CHUNK_BYTES / 2,
        );
        bytes.push(b'\n');
        expected.push('\n');
        append_euc_kr_until(
            &mut bytes,
            &mut expected,
            LEGACY_ROUND_TRIP_BYTE_CHUNK_BYTES * 2 + LEGACY_DETECTION_SAMPLE_CHUNK_BYTES,
        );

        assert!(legacy_samples_need_full_round_trip(&bytes));

        let (text, encoding) = decode_bytes(&bytes, None).expect("decode sparse euc-kr");
        assert_eq!(text, expected);
        assert_eq!(encoding, TextEncoding::EucKr);
    }

    #[test]
    fn legacy_round_trip_compares_large_content_in_chunks() {
        let mut text = String::new();
        while text.len() <= ROUND_TRIP_COMPARE_TEXT_CHUNK_BYTES + SHIFT_JIS_CR_SAMPLE.text.len() {
            text.push_str(SHIFT_JIS_CR_SAMPLE.text);
        }
        let bytes = encode_text(&text, TextEncoding::ShiftJis).expect("encode shift-jis");

        assert!(round_trips(&bytes, &text, TextEncoding::ShiftJis));

        let mut changed = bytes.clone();
        let last = changed.len() - 1;
        changed[last] = changed[last].wrapping_add(1);
        assert!(!round_trips(&changed, &text, TextEncoding::ShiftJis));
    }

    #[test]
    fn save_output_preserves_loaded_sample_bytes_when_unchanged() {
        for sample in ENCODED_SAMPLES {
            let (text, encoding) = decode_bytes(sample.bytes, None).expect(sample.name);
            let line_ending = LineEnding::detect(&text);
            let bytes =
                encode_text(&line_ending.normalize_text(&text), encoding).expect(sample.name);

            assert_eq!(bytes, sample.bytes, "{}", sample.name);
        }
    }

    #[test]
    fn saved_document_byte_len_uses_written_normalized_encoding_output() -> Result<(), AppError> {
        let root = TempRoot::new("j3text-saved-byte-len-from-written-output");
        let io = FileDocumentIo::new();
        let utf16_path = root.path().join("utf16.txt");
        let utf8_bom_path = root.path().join("utf8-bom.txt");

        let utf16_snapshot = io.save_with_target_expectation(
            &utf16_path,
            "a\n😀",
            TextEncoding::Utf16Le,
            LineEnding::Crlf,
            SaveTargetExpectation::Missing,
        )?;
        let utf8_bom_snapshot = io.save_with_target_expectation(
            &utf8_bom_path,
            "a\nb",
            TextEncoding::Utf8Bom,
            LineEnding::Crlf,
            SaveTargetExpectation::Missing,
        )?;

        assert_eq!(utf16_snapshot.byte_len, 12);
        assert_eq!(
            fs::metadata(&utf16_path)
                .expect("utf-16 saved metadata")
                .len(),
            12
        );
        assert_eq!(utf8_bom_snapshot.byte_len, 7);
        assert_eq!(
            fs::metadata(&utf8_bom_path)
                .expect("utf-8 bom saved metadata")
                .len(),
            7
        );
        assert_no_temp_save_files(root.path());
        Ok(())
    }

    #[test]
    fn saved_document_byte_len_rejects_reopen_limit() {
        let error = validate_saved_document_byte_len_with_limit(Path::new("oversized.txt"), 11, 10)
            .expect_err("oversized saved output should fail");

        assert_file_too_large(error, 11, 10);
    }

    #[test]
    fn encoded_saved_document_limit_is_checked_from_written_bytes() {
        let root = TempRoot::new("j3text-streaming-saved-byte-limit");
        let path = root.path().join("oversized.txt");

        let error = atomic_writer::write_encoded_text_with_limit(
            &path,
            "a\n😀",
            TextEncoding::Utf16Le,
            LineEnding::Crlf,
            SaveTargetExpectation::Missing,
            InitialTargetCheck::Required,
            11,
        )
        .expect_err("oversized normalized encoded output should fail");

        assert_file_too_large(error, 12, 11);
        assert!(!path.exists());
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn read_document_bytes_reports_actual_bytes_and_fingerprint() -> Result<(), AppError> {
        let root = TempRoot::new("j3text-read-document-bytes");
        let path = root.path().join("note.txt");
        let bytes = b"actual\r\nbytes";
        fs::write(&path, bytes).expect("write document");

        let read = read_document_bytes(&path, None, None)?;

        assert_eq!(read.bytes, bytes);
        assert_eq!(read.byte_len, bytes.len() as u64);
        assert_eq!(read.content_fingerprint, content_fingerprint(bytes));
        Ok(())
    }

    #[test]
    fn file_load_metadata_preserves_analysis_from_decoded_text() -> Result<(), AppError> {
        let root = TempRoot::new("j3text-load-analysis");
        let path = root.path().join("note.txt");
        let text = "a한\r\n😀\r\n끝\n";
        fs::write(&path, text.as_bytes()).expect("write document");

        let loaded = document_repository::load_with_metadata(&path, None, None)?;
        assert_eq!(loaded.document.content, text);
        assert_eq!(loaded.document.encoding, TextEncoding::Utf8);
        assert_eq!(loaded.document.line_ending, LineEnding::Crlf);

        let document =
            Document::from_loaded_with_metrics(DocumentId::new(1), loaded.document, loaded.metrics);
        assert_eq!(document.char_count(), 9);
        Ok(())
    }

    #[test]
    fn forced_utf8_keeps_bom_as_text_for_encoding_correction() {
        let bytes = b"\xEF\xBB\xBFalpha";
        let (text, encoding) = decode_bytes(bytes, Some(TextEncoding::Utf8)).expect("forced utf-8");

        assert_eq!(encoding, TextEncoding::Utf8);
        assert_eq!(text, "\u{feff}alpha");
        assert_eq!(encode_text(&text, encoding).expect("encode"), bytes);
    }

    #[test]
    fn forced_encoding_bypasses_auto_detection() {
        let (text, encoding) = decode_bytes(UTF8_BOM_LF_SAMPLE.bytes, Some(TextEncoding::Iso88591))
            .expect("forced iso-8859-1");

        assert_eq!(encoding, TextEncoding::Iso88591);
        assert_eq!(text, "ï»¿alpha\nbeta");
    }

    #[test]
    fn invalid_bomless_utf8_auto_detects_legacy_fallback() {
        let (text, encoding) = decode_bytes(b"caf\xE9", None).expect("auto legacy fallback");

        assert_eq!(text, "café");
        assert_eq!(encoding, TextEncoding::Iso88591);
    }

    #[test]
    fn forced_utf8_decode_failure_does_not_fallback() {
        let error =
            decode_bytes(b"caf\xE9", Some(TextEncoding::Utf8)).expect_err("forced utf-8 fails");

        assert_eq!(error.encoding_error_kind(), Some(EncodingErrorKind::Decode));
        assert!(error.to_string().contains("Invalid UTF-8 input"));
        assert!(error.user_message().contains("read"));
    }

    #[test]
    fn utf8_bom_with_invalid_payload_fails_before_fallback() {
        let error = decode_bytes(b"\xEF\xBB\xBF\xFF", None).expect_err("invalid utf-8 bom fails");

        assert_eq!(error.encoding_error_kind(), Some(EncodingErrorKind::Decode));
        assert!(error.to_string().contains("Invalid UTF-8 BOM input"));
    }

    #[test]
    fn bomless_utf16_samples_are_detected_before_utf8_fallback() {
        for (bytes, expected_text, expected_encoding) in [
            (
                b"A\x00\n\x00B\x00".as_slice(),
                "A\nB",
                TextEncoding::Utf16Le,
            ),
            (
                b"\x00A\x00\r\x00B".as_slice(),
                "A\rB",
                TextEncoding::Utf16Be,
            ),
        ] {
            let (text, encoding) = decode_bytes(bytes, None).expect("decode bomless utf-16");

            assert_eq!(text, expected_text);
            assert_eq!(encoding, expected_encoding);
            assert!(!text.contains('\0'));
        }
    }

    #[test]
    fn large_bomless_utf16_uses_sampled_detection() {
        let mut bytes = Vec::new();
        while bytes.len() <= BOMLESS_UTF16_DETECTION_FULL_SCAN_BYTES {
            bytes.extend_from_slice(b"A\x00");
        }

        assert!(looks_like_utf16(&bytes, true));
        assert!(!looks_like_utf16(&bytes, false));

        let (text, encoding) = decode_bytes(&bytes, None).expect("decode large bomless utf-16");
        assert_eq!(encoding, TextEncoding::Utf16Le);
        assert_eq!(text.len(), bytes.len() / 2);
        assert!(text.chars().all(|ch| ch == 'A'));
    }

    #[test]
    fn nul_text_content_is_rejected_after_decoding() {
        for (name, bytes, requested_encoding) in [
            ("utf-8 auto", b"alpha\0beta".as_slice(), None),
            (
                "utf-8 forced",
                b"alpha\0beta".as_slice(),
                Some(TextEncoding::Utf8),
            ),
            ("utf-16 le", b"\xFF\xFEA\x00\x00\x00B\x00".as_slice(), None),
            ("utf-16 be", b"\xFE\xFF\x00A\x00\x00\x00B".as_slice(), None),
        ] {
            let error = decode_bytes(bytes, requested_encoding).expect_err(name);

            assert!(matches!(error, AppError::Encoding { .. }), "{name}");
            assert_eq!(
                error.encoding_error_kind(),
                Some(EncodingErrorKind::UnsafeText),
                "{name}"
            );
            assert!(error.user_message().contains("NUL"), "{name}");
        }
    }

    #[test]
    fn file_load_rejects_nul_from_deferred_text_scan() {
        let root = TempRoot::new("j3text-load-nul");
        let path = root.path().join("nul.txt");
        fs::write(&path, b"alpha\0beta\r\n").expect("write document");

        let error = match document_repository::load_with_metadata(&path, None, None) {
            Ok(_) => panic!("load should reject nul"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Encoding { .. }));
        assert_eq!(
            error.encoding_error_kind(),
            Some(EncodingErrorKind::UnsafeText)
        );
        assert!(error.user_message().contains("NUL"));
    }

    #[test]
    fn file_load_save_reload_preserves_text_encoding_and_line_ending() {
        let root = env::temp_dir().join(format!("j3text-encoding-test-{}", save_token()));
        fs::create_dir_all(&root).expect("create temp dir");
        let io = FileDocumentIo::new();

        for sample in ENCODED_SAMPLES {
            let path = root.join(format!("{}.txt", sample.name.replace(' ', "-")));
            fs::write(&path, sample.bytes).expect("write sample");

            let loaded = io.load(&path, None, None).expect(sample.name);
            assert_eq!(loaded.content, sample.text, "{}", sample.name);
            assert_eq!(loaded.encoding, sample.encoding, "{}", sample.name);
            assert_eq!(loaded.line_ending, sample.line_ending, "{}", sample.name);

            io.save(
                &path,
                &loaded.content,
                loaded.encoding,
                loaded.line_ending,
                loaded.snapshot,
            )
            .expect(sample.name);

            assert_eq!(fs::read(&path).expect("read saved sample"), sample.bytes);
            let reloaded = io.load(&path, None, None).expect(sample.name);
            assert_eq!(reloaded.content, loaded.content, "{}", sample.name);
            assert_eq!(reloaded.encoding, loaded.encoding, "{}", sample.name);
            assert_eq!(reloaded.line_ending, loaded.line_ending, "{}", sample.name);
        }

        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn save_as_different_encoding_changes_bytes_and_reloads() {
        let text = "café\r\n";
        let bytes = encode_text(text, TextEncoding::Iso88591).expect("encode iso-8859-1");

        assert_eq!(bytes, b"caf\xE9\r\n");
        let (reloaded, encoding) =
            decode_bytes(&bytes, Some(TextEncoding::Iso88591)).expect("decode iso-8859-1");
        assert_eq!(reloaded, text);
        assert_eq!(encoding, TextEncoding::Iso88591);
    }

    #[test]
    fn file_save_as_different_encoding_writes_target_bytes_and_reloads() {
        let root = TempRoot::new("j3text-save-as-encoding");
        let io = FileDocumentIo::new();
        let latin_path = root.path().join("latin1.txt");
        let shift_jis_path = root.path().join("shift-jis.txt");

        io.save(
            &latin_path,
            "café\n",
            TextEncoding::Iso88591,
            LineEnding::Lf,
            None,
        )
        .expect("save latin1");
        assert_eq!(
            fs::read(&latin_path).expect("read latin1 bytes"),
            b"caf\xE9\n"
        );
        let latin = io
            .load(&latin_path, Some(TextEncoding::Iso88591), None)
            .expect("reload latin1");
        assert_eq!(latin.content, "café\n");
        assert_eq!(latin.encoding, TextEncoding::Iso88591);
        assert_eq!(latin.line_ending, LineEnding::Lf);

        io.save(
            &shift_jis_path,
            "日本語\r",
            TextEncoding::ShiftJis,
            LineEnding::Cr,
            None,
        )
        .expect("save shift-jis");
        assert_eq!(
            fs::read(&shift_jis_path).expect("read shift-jis bytes"),
            b"\x93\xFA\x96\x7B\x8C\xEA\r"
        );
        let shift_jis = io
            .load(&shift_jis_path, Some(TextEncoding::ShiftJis), None)
            .expect("reload shift-jis");
        assert_eq!(shift_jis.content, "日本語\r");
        assert_eq!(shift_jis.encoding, TextEncoding::ShiftJis);
        assert_eq!(shift_jis.line_ending, LineEnding::Cr);
    }

    #[test]
    fn file_reopen_with_requested_encoding_corrects_auto_detection() {
        let root = TempRoot::new("j3text-reopen-forced-encoding");
        let path = root.path().join("utf8-bom.txt");
        fs::write(&path, UTF8_BOM_LF_SAMPLE.bytes).expect("write utf8 bom sample");
        let io = FileDocumentIo::new();

        let auto = io.load(&path, None, None).expect("auto load utf8 bom");
        assert_eq!(auto.content, "alpha\nbeta");
        assert_eq!(auto.encoding, TextEncoding::Utf8Bom);

        let forced = io
            .load(&path, Some(TextEncoding::Iso88591), None)
            .expect("forced iso-8859-1 load");
        assert_eq!(forced.content, "ï»¿alpha\nbeta");
        assert_eq!(forced.encoding, TextEncoding::Iso88591);
    }

    #[test]
    fn ensure_encodable_reports_unrepresentable_text_before_save() {
        let io = FileDocumentIo::new();

        assert!(io.ensure_encodable("café", TextEncoding::Iso88591).is_ok());
        let error = io
            .ensure_encodable("cannot save 한", TextEncoding::Iso88591)
            .expect_err("iso-8859-1 warning boundary");

        assert!(matches!(error, AppError::Encoding { .. }));
        assert_eq!(error.encoding_error_kind(), Some(EncodingErrorKind::Encode));
        assert!(error.user_message().contains("saved"));
        assert!(error.to_string().contains("ISO-8859-1"));
    }

    #[test]
    fn ensure_encodable_checks_code_page_text_past_chunk_boundary() {
        let io = FileDocumentIo::new();
        let mut text = "a".repeat(WRITE_CHUNK_BYTES + 8);
        text.push('한');

        let error = match io.ensure_encodable(&text, TextEncoding::ShiftJis) {
            Ok(()) => panic!("shift-jis should reject unrepresentable text past chunk boundary"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Encoding { .. }));
        assert_eq!(error.encoding_error_kind(), Some(EncodingErrorKind::Encode));
        assert!(error.user_message().contains("saved"));
    }

    #[test]
    fn iso_8859_1_rejects_unrepresentable_text() {
        assert!(encode_text("é", TextEncoding::Iso88591).is_ok());
        assert!(encode_text("한", TextEncoding::Iso88591).is_err());
    }

    #[test]
    fn code_page_encoders_reject_unrepresentable_text() {
        assert!(encode_text("한글", TextEncoding::EucKr).is_ok());
        assert!(encode_text("뷁", TextEncoding::EucKr).is_err());
        assert!(encode_text("日本語", TextEncoding::ShiftJis).is_ok());
        assert!(encode_text("한글", TextEncoding::ShiftJis).is_err());
    }

    #[test]
    fn added_code_page_encodings_round_trip_when_forced() {
        for (encoding, text) in [
            (TextEncoding::Gb18030, "中文"),
            (TextEncoding::Big5, "繁體中文"),
            (TextEncoding::Windows1250, "Zażółć"),
            (TextEncoding::Windows1251, "Привет"),
            (TextEncoding::Windows1252, "café €"),
            (TextEncoding::Windows1253, "Καλημέρα"),
            (TextEncoding::Windows1254, "İstanbul"),
            (TextEncoding::Windows1255, "שלום"),
            (TextEncoding::Windows1256, "سلام"),
            (TextEncoding::Windows1257, "Āžu"),
            (TextEncoding::Windows874, "ภาษาไทย"),
        ] {
            let bytes = encode_text(text, encoding)
                .unwrap_or_else(|_| panic!("{}", encoding.display_name()));
            let (decoded, detected) = decode_bytes(&bytes, Some(encoding))
                .unwrap_or_else(|_| panic!("{}", encoding.display_name()));

            assert_eq!(decoded, text, "{}", encoding.display_name());
            assert_eq!(detected, encoding, "{}", encoding.display_name());
        }
    }

    #[test]
    fn escaped_values_round_trip() {
        let value = "C:\\Temp\\a%file\r\n.txt";
        assert_eq!(unescape_value(&escape_value(value)), value);
    }

    #[test]
    fn settings_shortcuts_round_trip_and_can_disable() {
        let root = TempRoot::new("j3text-settings-shortcuts");
        let store = UserDataStore::with_root(root.path().join("user-data")).expect("create store");
        let mut settings = EditorSettings {
            font_name: "Mono \"Quoted\"".to_string(),
            font_size_pt: 14,
            theme: ThemeMode::SteelBlue,
            ..EditorSettings::default()
        };
        settings.shortcuts.close_tab = Some(KeyboardShortcut::CTRL_F4);
        settings.shortcuts.save_as = None;
        settings.shortcuts.find = Some(KeyboardShortcut::CTRL_SHIFT_F);

        store.save_settings(&settings).expect("save settings");
        let saved_text = fs::read_to_string(store.settings_path()).expect("read saved settings");
        let saved_table = saved_text
            .parse::<toml::Table>()
            .expect("saved settings are TOML");
        assert_eq!(
            saved_table.get("font_name").and_then(toml::Value::as_str),
            Some("Mono \"Quoted\"")
        );
        assert_eq!(
            saved_table
                .get("font_size_pt")
                .and_then(toml::Value::as_integer),
            Some(14)
        );
        assert_eq!(
            saved_table.get("theme").and_then(toml::Value::as_str),
            Some("steel-blue")
        );
        assert_eq!(
            saved_table
                .get("shortcut_close_tab")
                .and_then(toml::Value::as_str),
            Some("ctrl+f4")
        );
        let reloaded = store.load_settings().expect("load settings");
        assert_eq!(reloaded.font_name, "Mono \"Quoted\"");
        assert_eq!(reloaded.font_size_pt, 14);
        assert_eq!(reloaded.theme, ThemeMode::SteelBlue);
        assert_eq!(
            reloaded.shortcuts.close_tab,
            Some(KeyboardShortcut::CTRL_F4)
        );
        assert_eq!(reloaded.shortcuts.save_as, None);
        assert_eq!(
            reloaded.shortcuts.find,
            Some(KeyboardShortcut::CTRL_SHIFT_F)
        );

        settings.shortcuts.close_tab = None;
        store
            .save_settings(&settings)
            .expect("save disabled shortcut");
        let reloaded = store.load_settings().expect("load disabled shortcut");
        assert_eq!(reloaded.shortcuts.close_tab, None);
    }

    #[test]
    fn settings_loads_current_toml_only_and_ignores_removed_keys() {
        let root = TempRoot::new("j3text-settings-no-migration");
        let store = UserDataStore::with_root(root.path().join("user-data")).expect("create store");
        let default_settings = EditorSettings::default();

        fs::write(
            store.settings_path(),
            "font_name = LegacyMono\nfont_size_pt = 20\ntab_size = 8\n",
        )
        .expect("write previous settings format");
        let loaded = store.load_settings().expect("load unsupported settings");
        assert_eq!(loaded.font_name, default_settings.font_name);
        assert_eq!(loaded.font_size_pt, default_settings.font_size_pt);
        assert_eq!(loaded.tab_size, default_settings.tab_size);

        fs::write(
            store.settings_path(),
            r#"
font_name = "CurrentMono"
font_size_pt = 13
tab_size = 2
auto_save = false
"#,
        )
        .expect("write current settings with removed key");
        let loaded = store
            .load_settings()
            .expect("load current settings with removed key");
        assert_eq!(loaded.font_name, "CurrentMono");
        assert_eq!(loaded.font_size_pt, 13);
        assert_eq!(loaded.tab_size, 2);
    }

    #[test]
    fn settings_path_uses_executable_name_with_toml_extension() {
        assert_eq!(
            settings_path_for_executable_path(PathBuf::from("j3text.exe"))
                .expect("settings path")
                .file_name()
                .and_then(|name| name.to_str()),
            Some("j3text.toml")
        );
        assert_eq!(
            settings_path_for_executable_path(PathBuf::from("j3text"))
                .expect("settings path")
                .file_name()
                .and_then(|name| name.to_str()),
            Some("j3text.toml")
        );
        assert_eq!(
            settings_path_for_executable_path(PathBuf::from("app/bin/j3text.exe"))
                .expect("settings path"),
            PathBuf::from("app/bin/j3text.toml")
        );
        assert_eq!(
            settings_path_for_executable_path(PathBuf::from("app/bin/j3text"))
                .expect("settings path"),
            PathBuf::from("app/bin/j3text.toml")
        );
    }

    #[test]
    fn injected_user_data_root_keeps_settings_file_name_rule() {
        let root = TempRoot::new("j3text-settings-path");
        let store_root = root.path().join("user-data");
        let store = UserDataStore::with_root(store_root.clone()).expect("create store");
        let path = store.settings_path();

        assert_eq!(path.parent(), Some(store_root.as_path()));
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some(default_settings_file_name())
        );
        assert_eq!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("toml")
        );
    }

    #[test]
    fn oversized_settings_load_is_rejected_before_full_read() {
        let root = TempRoot::new("j3text-oversized-settings");
        let store = UserDataStore::with_root(root.path().join("user-data")).expect("create store");
        let path = store.settings_path();
        let file = File::create(&path).expect("create oversized settings");
        file.set_len(USER_DATA_TEXT_LOAD_LIMIT_BYTES + 1)
            .expect("extend oversized settings");

        let error = store
            .load_settings()
            .expect_err("oversized settings should fail");

        assert_file_too_large(
            error,
            USER_DATA_TEXT_LOAD_LIMIT_BYTES + 1,
            USER_DATA_TEXT_LOAD_LIMIT_BYTES,
        );
    }

    #[test]
    fn save_tokens_are_unique_for_rapid_successive_saves() {
        let mut tokens = HashSet::new();

        for _ in 0..64 {
            assert!(tokens.insert(save_token()));
        }
    }

    #[test]
    fn file_document_save_replaces_existing_content() {
        let root = env::temp_dir().join(format!("j3text-save-test-{}", save_token()));
        fs::create_dir_all(&root).expect("create temp dir");
        let path = root.join("note.txt");
        fs::write(&path, "old").expect("write old content");

        let io = FileDocumentIo::new();
        io.save(
            &path,
            "new\nline",
            TextEncoding::Utf8,
            LineEnding::Crlf,
            None,
        )
        .expect("save atomically");

        let saved = fs::read_to_string(&path).expect("read saved content");
        assert_eq!(saved, "new\r\nline");
        fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn streamed_utf8_save_normalizes_large_mixed_line_endings() {
        let root = TempRoot::new("j3text-streamed-utf8-save");
        let path = root.path().join("large.txt");
        fs::write(&path, "old").expect("write old content");
        let mut text = String::new();
        for index in 0..4096 {
            text.push_str("한글 ");
            text.push_str(&index.to_string());
            text.push_str("\r\nline\nnext\rlast\n");
        }

        let io = FileDocumentIo::new();
        let snapshot = io
            .save(&path, &text, TextEncoding::Utf8, LineEnding::Lf, None)
            .expect("save streamed utf-8");

        let saved = fs::read_to_string(&path).expect("read saved content");
        assert!(!saved.contains('\r'));
        assert!(saved.contains("한글 4095\nline\nnext\nlast\n"));
        assert_eq!(
            snapshot.byte_len,
            fs::metadata(&path).expect("metadata").len()
        );
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn utf8_save_normalizes_suffix_from_crlf_boundary_for_cr_target() -> Result<(), AppError> {
        let mut saved = Vec::new();

        write_utf8_normalized(
            &mut saved,
            "prefix\r\nsuffix\nlast",
            LineEnding::Cr,
            Path::new("utf8-normalized.tmp"),
        )?;

        assert_eq!(saved, b"prefix\rsuffix\rlast");
        Ok(())
    }

    #[test]
    fn streamed_encoding_failure_keeps_original_and_removes_temp_file() {
        let root = TempRoot::new("j3text-streamed-encoding-failure");
        let path = root.path().join("latin1.txt");
        fs::write(&path, b"original").expect("write old content");
        let before = fs::read(&path).expect("read before");

        let io = FileDocumentIo::new();
        let error = io
            .save(
                &path,
                "cannot save 한",
                TextEncoding::Iso88591,
                LineEnding::Lf,
                None,
            )
            .expect_err("iso-8859-1 save should fail");

        assert!(matches!(error, AppError::Encoding { .. }));
        assert_eq!(fs::read(&path).expect("read after"), before);
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn streamed_code_page_failure_keeps_original_and_removes_temp_file() {
        let root = TempRoot::new("j3text-streamed-code-page-failure");
        let path = root.path().join("shift-jis.txt");
        fs::write(&path, b"original").expect("write old content");
        let before = fs::read(&path).expect("read before");
        let mut text = String::new();
        for _ in 0..2048 {
            text.push_str("日本語\r\n");
        }
        text.push_str("한글 cannot be saved as shift-jis");

        let io = FileDocumentIo::new();
        let error = io
            .save(&path, &text, TextEncoding::ShiftJis, LineEnding::Lf, None)
            .expect_err("shift-jis save should fail");

        assert!(matches!(error, AppError::Encoding { .. }));
        assert_eq!(fs::read(&path).expect("read after"), before);
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn forced_large_file_load_marks_document_read_only() {
        let root = TempRoot::new("j3text-forced-large-read-only");
        let path = root.path().join("large-policy.txt");
        fs::write(&path, b"large policy content").expect("write file");

        let loaded = FileDocumentIo::new()
            .load(&path, None, Some(ReadOnlyReason::LargeFile))
            .expect("load with large-file policy");

        assert_eq!(loaded.read_only_reason, Some(ReadOnlyReason::LargeFile));
        assert_eq!(loaded.content, "large policy content");
    }

    #[test]
    fn oversized_document_load_is_rejected_before_full_read() {
        let root = TempRoot::new("j3text-oversized-load-rejected");
        let path = root.path().join("oversized.txt");
        let file = File::create(&path).expect("create oversized file");
        file.set_len(MAX_DOCUMENT_LOAD_BYTES + 1)
            .expect("extend oversized file");

        let error = match FileDocumentIo::new().load(&path, None, None) {
            Ok(_) => panic!("oversized load should fail"),
            Err(error) => error,
        };

        match &error {
            AppError::FileTooLarge {
                byte_len, limit, ..
            } => {
                assert_eq!(*byte_len, MAX_DOCUMENT_LOAD_BYTES + 1);
                assert_eq!(*limit, MAX_DOCUMENT_LOAD_BYTES);
            }
            _ => panic!("expected file-too-large error"),
        }
        assert!(error.user_message().contains("too large"));
    }

    #[cfg(windows)]
    #[test]
    fn recent_files_preserve_non_utf8_windows_paths() {
        let root = TempRoot::new("j3text-recent-files-non-utf8-path");
        let store = UserDataStore::with_root(root.path().join("user-data")).expect("create store");
        let recent_path = windows_path_with_unpaired_surrogate("recent");

        store
            .save_recent_files(std::slice::from_ref(&recent_path))
            .expect("save recent files");
        let restored = store.load_recent_files().expect("load recent files");

        assert_eq!(restored, vec![recent_path]);
    }

    #[test]
    fn oversized_recent_files_load_is_rejected_before_full_read() {
        let root = TempRoot::new("j3text-oversized-recent-files");
        let store = UserDataStore::with_root(root.path().join("user-data")).expect("create store");
        let path = store.recent_files_path();
        let file = File::create(&path).expect("create oversized recent files");
        file.set_len(USER_DATA_TEXT_LOAD_LIMIT_BYTES + 1)
            .expect("extend oversized recent files");

        let error = store
            .load_recent_files()
            .expect_err("oversized recent files should fail");

        assert_file_too_large(
            error,
            USER_DATA_TEXT_LOAD_LIMIT_BYTES + 1,
            USER_DATA_TEXT_LOAD_LIMIT_BYTES,
        );
    }

    #[test]
    fn file_io_integration_save_as_recent_files_round_trip() {
        let root = TempRoot::new("j3text-file-io-integration");
        let io = FileDocumentIo::new();
        let mut app = EditorApp::new();
        app.new_document();
        app.update_current_content("alpha\nbeta".to_string())
            .expect("edit new document");

        let save_path = root.path().join("draft.txt");
        let document = app.current_document().expect("current document");
        let snapshot = io
            .save(
                &save_path,
                document.content(),
                document.encoding(),
                LineEnding::Lf,
                None,
            )
            .expect("save as new file");
        app.mark_current_saved(
            save_path.clone(),
            TextEncoding::Utf8,
            LineEnding::Lf,
            Some(snapshot),
        )
        .expect("mark save as complete");

        assert_eq!(
            fs::read(&save_path).expect("read saved file"),
            b"alpha\nbeta"
        );
        assert!(
            !app.current_document()
                .expect("current document")
                .dirty_state()
                .is_dirty()
        );
        assert_eq!(app.recent_files(), std::slice::from_ref(&save_path));

        let opened = io.load(&save_path, None, None).expect("open saved file");
        assert_eq!(opened.content, "alpha\nbeta");
        assert_eq!(opened.encoding, TextEncoding::Utf8);
        assert_eq!(opened.line_ending, LineEnding::Lf);

        let second_path = root.path().join("second.txt");
        fs::write(&second_path, b"second\r\nfile").expect("write second file");
        let second = io.load(&second_path, None, None).expect("open second file");
        app.open_document(second);

        let store_root = root.path().join("user-data");
        let store = UserDataStore::with_root(store_root).expect("create user data store");
        store
            .save_recent_files(app.recent_files())
            .expect("save recent files");
        let restored = store.load_recent_files().expect("load recent files");

        assert_eq!(
            restored.iter().map(PathBuf::as_path).collect::<Vec<_>>(),
            vec![second_path.as_path(), save_path.as_path()]
        );
    }

    #[test]
    fn external_modification_is_detected_and_reload_reads_new_bytes() {
        let root = TempRoot::new("j3text-external-change");
        let io = FileDocumentIo::new();
        let path = root.path().join("note.txt");
        fs::write(&path, b"old\r\n").expect("write old file");

        let loaded = io.load(&path, None, None).expect("load old file");
        let previous = loaded.snapshot.expect("initial file snapshot");
        let before_bytes = fs::read(&path).expect("read before bytes");

        fs::write(&path, b"new external bytes\r\n").expect("write external change");
        let current = io.file_snapshot(&path).expect("snapshot after change");
        let after_bytes = fs::read(&path).expect("read after bytes");

        assert_ne!(before_bytes, after_bytes);
        assert!(
            previous.has_changed_from(current),
            "metadata did not detect byte/time change: before={previous:?}, after={current:?}"
        );

        let reloaded = io.load(&path, None, None).expect("reload external change");
        assert_eq!(reloaded.content, "new external bytes\r\n");
        assert_eq!(reloaded.snapshot, Some(current));
    }

    #[test]
    fn save_with_stale_snapshot_rejects_external_change_and_preserves_file() {
        let root = TempRoot::new("j3text-save-conflict");
        let io = FileDocumentIo::new();
        let path = root.path().join("note.txt");
        fs::write(&path, b"original\r\n").expect("write original file");

        let loaded = io.load(&path, None, None).expect("load original file");
        let previous = loaded.snapshot.expect("initial file snapshot");

        fs::write(&path, b"external replacement\r\n").expect("write external change");
        let external_bytes = fs::read(&path).expect("read external bytes");

        let error = io
            .save(
                &path,
                "user draft\r\n",
                TextEncoding::Utf8,
                LineEnding::Crlf,
                Some(previous),
            )
            .expect_err("stale snapshot save should fail");

        assert!(matches!(error, AppError::ExternalFileChanged { .. }));
        assert_eq!(
            fs::read(&path).expect("read after failed save"),
            external_bytes
        );
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn save_with_stale_snapshot_rejects_different_size_large_change_without_fingerprint() {
        let root = TempRoot::new("j3text-save-large-conflict-no-fingerprint");
        let io = FileDocumentIo::new();
        let path = root.path().join("note.txt");
        fs::write(&path, b"original\r\n").expect("write original file");

        let loaded = io.load(&path, None, None).expect("load original file");
        let previous = loaded.snapshot.expect("initial file snapshot");
        let large_len = MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES + 1;
        let replacement = File::create(&path).expect("create large external replacement");
        replacement
            .set_len(large_len)
            .expect("extend large external replacement");

        reset_file_fingerprint_read_count();
        let error = io
            .save(
                &path,
                "user draft\r\n",
                TextEncoding::Utf8,
                LineEnding::Crlf,
                Some(previous),
            )
            .expect_err("large stale snapshot save should fail");

        assert!(matches!(error, AppError::ExternalFileChanged { .. }));
        assert_eq!(file_fingerprint_read_count(), 0);
        assert_eq!(
            fs::metadata(&path)
                .expect("metadata after failed save")
                .len(),
            large_len
        );
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn same_size_oversized_stale_snapshot_is_rejected_without_fingerprint() {
        let root = TempRoot::new("j3text-save-same-size-oversized-conflict");
        let path = root.path().join("note.txt");
        let large_len = MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES + 1;
        let file = File::create(&path).expect("create oversized file");
        file.set_len(large_len).expect("extend oversized file");
        let expected = FileSnapshot {
            modified: Some(UNIX_EPOCH),
            byte_len: large_len,
        };

        reset_file_fingerprint_read_count();
        let error = ensure_target_snapshot_matches(&path, Some(expected), None)
            .expect_err("oversized same-size stale snapshot should fail");

        assert!(matches!(error, AppError::ExternalFileChanged { .. }));
        assert_eq!(file_fingerprint_read_count(), 0);
    }

    #[test]
    fn save_with_unchanged_snapshot_reads_fingerprint_for_replace_check_only() {
        let root = TempRoot::new("j3text-save-fingerprint-count");
        let io = FileDocumentIo::new();
        let path = root.path().join("note.txt");
        fs::write(&path, b"original\r\n").expect("write original file");

        let loaded = io.load(&path, None, None).expect("load original file");
        let previous = loaded.snapshot.expect("initial file snapshot");

        reset_file_fingerprint_read_count();
        io.save(
            &path,
            "user draft\r\n",
            TextEncoding::Utf8,
            LineEnding::Crlf,
            Some(previous),
        )
        .expect("save unchanged target");

        assert_eq!(
            file_fingerprint_read_count(),
            1,
            "unchanged save should read the target fingerprint for the replace-time conflict check only"
        );
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn save_with_unchanged_metadata_skips_replace_check_fingerprint_read() {
        let root = TempRoot::new("j3text-save-metadata-skip-fingerprint");
        let io = FileDocumentIo::new();
        let path = root.path().join("note.txt");
        fs::write(&path, b"original\r\n").expect("write original file");

        let loaded = io.load(&path, None, None).expect("load original file");
        let previous = loaded.snapshot.expect("initial file snapshot");
        let metadata = io
            .file_metadata_snapshot(&path)
            .expect("initial file metadata");
        if !metadata.has_change_marker() {
            return;
        }

        reset_file_fingerprint_read_count();
        io.save_with_target_expectation(
            &path,
            "user draft\r\n",
            TextEncoding::Utf8,
            LineEnding::Crlf,
            SaveTargetExpectation::UnchangedWithMetadata {
                snapshot: previous,
                metadata,
            },
        )
        .expect("save unchanged target with metadata");

        #[cfg(windows)]
        let expected_fingerprint_reads = 0;
        #[cfg(not(windows))]
        let expected_fingerprint_reads = 1;
        assert_eq!(
            file_fingerprint_read_count(),
            expected_fingerprint_reads,
            "unchanged metadata should follow the platform change-marker policy"
        );
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn save_with_metadata_only_expectation_allows_large_existing_target() {
        let root = TempRoot::new("j3text-save-large-target-metadata-only");
        let io = FileDocumentIo::new();
        let path = root.path().join("note.txt");
        let file = File::create(&path).expect("create large target");
        file.set_len(MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES + 1)
            .expect("extend large target");
        let metadata = io
            .file_metadata_snapshot(&path)
            .expect("large target metadata");

        reset_file_fingerprint_read_count();
        io.save_with_target_expectation(
            &path,
            "user draft\n",
            TextEncoding::Utf8,
            LineEnding::Lf,
            SaveTargetExpectation::UnchangedMetadata(metadata),
        )
        .expect("save over unchanged large target");

        assert_eq!(fs::read(&path).expect("read saved target"), b"user draft\n");
        assert_eq!(file_fingerprint_read_count(), 0);
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn save_with_unchanged_metadata_falls_back_when_change_marker_is_missing() {
        let root = TempRoot::new("j3text-save-metadata-without-change-marker");
        let io = FileDocumentIo::new();
        let path = root.path().join("note.txt");
        fs::write(&path, b"original\r\n").expect("write original file");

        let loaded = io.load(&path, None, None).expect("load original file");
        let previous = loaded.snapshot.expect("initial file snapshot");
        let mut metadata = io
            .file_metadata_snapshot(&path)
            .expect("initial file metadata");
        metadata.change_time = None;

        reset_file_fingerprint_read_count();
        io.save_with_target_expectation(
            &path,
            "user draft\r\n",
            TextEncoding::Utf8,
            LineEnding::Crlf,
            SaveTargetExpectation::UnchangedWithMetadata {
                snapshot: previous,
                metadata,
            },
        )
        .expect("save unchanged target with incomplete metadata");

        assert_eq!(
            file_fingerprint_read_count(),
            1,
            "incomplete metadata should keep the target fingerprint conflict check"
        );
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn save_with_unchanged_metadata_rejects_same_size_external_change_without_fingerprint() {
        let root = TempRoot::new("j3text-save-metadata-same-size-conflict");
        let io = FileDocumentIo::new();
        let path = root.path().join("note.txt");
        fs::write(&path, b"same-len-old").expect("write original file");

        let loaded = io.load(&path, None, None).expect("load original file");
        let previous = loaded.snapshot.expect("initial file snapshot");
        let metadata = io
            .file_metadata_snapshot(&path)
            .expect("initial file metadata");
        if metadata.change_time.is_none() {
            return;
        }
        fs::write(&path, b"same-len-new").expect("write same-size external change");
        let external_bytes = fs::read(&path).expect("read external bytes");
        let current_metadata = io
            .file_metadata_snapshot(&path)
            .expect("changed file metadata");
        let expected_fingerprint_reads =
            if metadata.has_change_marker_changed_from(current_metadata) {
                0
            } else {
                1
            };

        reset_file_fingerprint_read_count();
        let error = io
            .save_with_target_expectation(
                &path,
                "user draft",
                TextEncoding::Utf8,
                LineEnding::Lf,
                SaveTargetExpectation::UnchangedWithMetadata {
                    snapshot: previous,
                    metadata,
                },
            )
            .expect_err("same-size external change should fail");

        assert!(matches!(error, AppError::ExternalFileChanged { .. }));
        assert_eq!(file_fingerprint_read_count(), expected_fingerprint_reads);
        assert_eq!(
            fs::read(&path).expect("read after failed save"),
            external_bytes
        );
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn save_without_expected_snapshot_does_not_read_saved_file_for_snapshot() {
        let root = TempRoot::new("j3text-save-without-post-write-read");
        let io = FileDocumentIo::new();
        let path = root.path().join("note.txt");
        let saved_bytes = b"user draft\r\n";

        reset_file_fingerprint_read_count();
        let snapshot = io
            .save(
                &path,
                "user draft\r\n",
                TextEncoding::Utf8,
                LineEnding::Crlf,
                None,
            )
            .expect("save new file");

        let modified = fs::metadata(&path)
            .expect("saved file metadata")
            .modified()
            .ok();
        assert_eq!(file_fingerprint_read_count(), 0);
        assert_eq!(snapshot.byte_len, saved_bytes.len() as u64);
        assert_eq!(
            snapshot.modified,
            snapshot_modified_marker(modified, content_fingerprint(saved_bytes))
        );
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn saved_file_snapshot_rejects_same_size_post_write_change() {
        let root = TempRoot::new("j3text-save-post-write-change");
        let path = root.path().join("note.txt");
        fs::write(&path, b"same-len-new").expect("write changed target");

        let error = saved_file_snapshot(
            &path,
            SavedContentFingerprint {
                written: WrittenContentFingerprint {
                    content_fingerprint: content_fingerprint(b"same-len-old"),
                    byte_len: b"same-len-old".len() as u64,
                },
                metadata: FileMetadataSnapshot {
                    modified: Some(UNIX_EPOCH),
                    byte_len: b"same-len-old".len() as u64,
                    change_time: None,
                },
            },
        )
        .expect_err("post-write target change should fail");

        assert!(matches!(error, AppError::ExternalFileChanged { .. }));
    }

    #[test]
    fn save_requiring_missing_target_recreates_absent_file() {
        let root = TempRoot::new("j3text-save-missing-target");
        let io = FileDocumentIo::new();
        let path = root.path().join("note.txt");
        fs::write(&path, b"original\r\n").expect("write original file");
        fs::remove_file(&path).expect("delete backing file");

        io.save_with_target_expectation(
            &path,
            "user draft\r\n",
            TextEncoding::Utf8,
            LineEnding::Crlf,
            SaveTargetExpectation::Missing,
        )
        .expect("save missing target");

        assert_eq!(
            fs::read(&path).expect("read recreated file"),
            b"user draft\r\n"
        );
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn save_requiring_missing_target_rejects_recreated_file() {
        let root = TempRoot::new("j3text-save-recreated-target-conflict");
        let io = FileDocumentIo::new();
        let path = root.path().join("note.txt");
        fs::write(&path, b"original\r\n").expect("write original file");
        fs::remove_file(&path).expect("delete backing file");
        fs::write(&path, b"external replacement\r\n").expect("recreate target");
        let external_bytes = fs::read(&path).expect("read external bytes");

        let error = io
            .save_with_target_expectation(
                &path,
                "user draft\r\n",
                TextEncoding::Utf8,
                LineEnding::Crlf,
                SaveTargetExpectation::Missing,
            )
            .expect_err("recreated target save should fail");

        assert!(matches!(error, AppError::ExternalFileChanged { .. }));
        assert_eq!(
            fs::read(&path).expect("read after failed save"),
            external_bytes
        );
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn snapshot_marker_changes_for_same_modified_time_content_change() {
        let modified = Some(SystemTime::now());

        assert_ne!(
            snapshot_modified_marker(modified, content_fingerprint(b"same-len-old")),
            snapshot_modified_marker(modified, content_fingerprint(b"same-len-new"))
        );
    }

    #[test]
    fn file_metadata_snapshot_change_time_difference_counts_as_changed() {
        let previous = FileMetadataSnapshot {
            modified: Some(UNIX_EPOCH),
            byte_len: 12,
            change_time: Some(10),
        };
        let current = FileMetadataSnapshot {
            modified: Some(UNIX_EPOCH),
            byte_len: 12,
            change_time: Some(11),
        };

        assert!(previous.has_change_marker_changed_from(current));
    }

    #[test]
    fn save_with_same_size_rejects_external_content_change() {
        let root = TempRoot::new("j3text-save-same-metadata-conflict");
        let io = FileDocumentIo::new();
        let path = root.path().join("note.txt");
        fs::write(&path, b"same-len-old").expect("write original file");

        let loaded = io.load(&path, None, None).expect("load original file");
        let previous = loaded.snapshot.expect("initial file snapshot");

        fs::write(&path, b"same-len-new").expect("write same-size external change");
        let current = io
            .file_snapshot(&path)
            .expect("snapshot after same-size change");
        let disguised_previous = FileSnapshot {
            modified: previous.modified,
            byte_len: current.byte_len,
        };
        let external_bytes = fs::read(&path).expect("read external bytes");

        assert_eq!(disguised_previous.byte_len, current.byte_len);
        assert!(disguised_previous.has_changed_from(current));

        let error = io
            .save(
                &path,
                "user draft",
                TextEncoding::Utf8,
                LineEnding::Lf,
                Some(disguised_previous),
            )
            .expect_err("same-metadata external change should fail");

        assert!(matches!(error, AppError::ExternalFileChanged { .. }));
        assert_eq!(
            fs::read(&path).expect("read after failed save"),
            external_bytes
        );
        assert_no_temp_save_files(root.path());
    }

    #[test]
    fn deleted_and_moved_file_paths_are_reported_as_missing() {
        let root = TempRoot::new("j3text-missing-paths");
        let io = FileDocumentIo::new();

        let deleted_path = root.path().join("deleted.txt");
        fs::write(&deleted_path, b"delete me").expect("write deleted file");
        let deleted = io
            .load(&deleted_path, None, None)
            .expect("load deleted file");
        assert!(deleted.snapshot.is_some());
        fs::remove_file(&deleted_path).expect("delete backing file");

        let deleted_error = io
            .file_snapshot(&deleted_path)
            .expect_err("deleted path should be missing");
        assert_eq!(
            deleted_error.file_access_kind(),
            Some(FileAccessKind::NotFound)
        );

        let moved_from = root.path().join("moved.txt");
        let moved_to = root.path().join("renamed.txt");
        fs::write(&moved_from, b"move me").expect("write moved file");
        let moved = io.load(&moved_from, None, None).expect("load moved file");
        assert!(moved.snapshot.is_some());
        fs::rename(&moved_from, &moved_to).expect("move backing file");

        let moved_error = io
            .file_snapshot(&moved_from)
            .expect_err("old moved path should be missing");
        assert_eq!(
            moved_error.file_access_kind(),
            Some(FileAccessKind::NotFound)
        );
        let reopened = io.load(&moved_to, None, None).expect("open moved file");
        assert_eq!(reopened.content, "move me");
    }

    #[test]
    fn read_only_file_loads_read_only_and_save_preserves_original_bytes() {
        let root = TempRoot::new("j3text-read-only");
        let io = FileDocumentIo::new();
        let path = root.path().join("readonly.txt");
        fs::write(&path, b"original").expect("write original");
        let mut permissions = fs::metadata(&path).expect("metadata").permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).expect("set read-only");

        let loaded = io.load(&path, None, None).expect("load read-only file");
        assert_eq!(loaded.read_only_reason, Some(ReadOnlyReason::FileAttribute));
        let before_bytes = fs::read(&path).expect("read before");
        let before_snapshot = io.file_snapshot(&path).expect("snapshot before");

        let error = io
            .save(
                &path,
                "replacement",
                TextEncoding::Utf8,
                LineEnding::Crlf,
                Some(before_snapshot),
            )
            .expect_err("read-only save should fail");

        assert_eq!(error.file_access_kind(), Some(FileAccessKind::ReadOnly));
        assert_eq!(fs::read(&path).expect("read after"), before_bytes);
        assert_eq!(
            io.file_snapshot(&path).expect("snapshot after"),
            before_snapshot
        );
    }

    #[cfg(windows)]
    #[test]
    fn atomic_save_failure_keeps_original_and_removes_temp_file() {
        let root = TempRoot::new("j3text-atomic-failure");
        let io = FileDocumentIo::new();
        let path = root.path().join("locked.txt");
        fs::write(&path, b"original").expect("write original");
        let before_bytes = fs::read(&path).expect("read before");
        let before_snapshot = io.file_snapshot(&path).expect("snapshot before");
        let locked = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&path)
            .expect("lock target file");

        let error = io
            .save(
                &path,
                "replacement",
                TextEncoding::Utf8,
                LineEnding::Crlf,
                Some(before_snapshot),
            )
            .expect_err("locked target replacement should fail");
        drop(locked);

        assert!(matches!(
            error.file_access_kind(),
            Some(FileAccessKind::FileInUse | FileAccessKind::PermissionDenied)
        ));
        assert_eq!(fs::read(&path).expect("read after"), before_bytes);
        assert_eq!(
            io.file_snapshot(&path).expect("snapshot after"),
            before_snapshot
        );
        assert_no_temp_save_files(root.path());
    }

    #[cfg(windows)]
    #[test]
    fn recent_files_save_failure_keeps_previous_recent_files_file() {
        let root = TempRoot::new("j3text-recent-files-atomic-failure");
        let store_root = root.path().join("user-data");
        let store = UserDataStore::with_root(store_root).expect("create user data store");
        let old_path = root.path().join("old.txt");
        let new_path = root.path().join("new.txt");
        store
            .save_recent_files(&[old_path])
            .expect("save initial recent files");
        let before_bytes = fs::read(store.recent_files_path()).expect("read initial recent files");
        let locked = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(store.recent_files_path())
            .expect("lock recent files file");

        let error = store
            .save_recent_files(&[new_path])
            .expect_err("locked recent files replacement should fail");
        drop(locked);

        assert!(matches!(
            error.file_access_kind(),
            Some(FileAccessKind::FileInUse | FileAccessKind::PermissionDenied)
        ));
        assert_eq!(
            fs::read(store.recent_files_path()).expect("read recent files after failed save"),
            before_bytes
        );
        assert_no_temp_save_files(store.user_data_root());
    }

    #[test]
    fn user_data_save_failure_does_not_mutate_open_document_state() {
        let root = TempRoot::new("j3text-user-data-failure");
        let store_root_file = root.path().join("not-a-directory");
        fs::write(&store_root_file, b"not a directory").expect("write file root");
        let store = UserDataStore {
            paths: UserDataPaths {
                user_data_root: store_root_file,
                settings_file: root.path().join("settings.toml"),
            },
        };
        let mut app = EditorApp::new();
        app.new_document();
        app.update_current_content("unsaved draft".to_string())
            .expect("edit document");
        let before_title = app
            .current_document()
            .expect("current document")
            .tab_title();
        let before_recent = app.recent_files().to_vec();

        let error = store
            .save_recent_files(app.recent_files())
            .expect_err("recent files save should fail");

        assert!(error.file_access_kind().is_some());
        let document = app.current_document().expect("current document");
        assert_eq!(document.content(), "unsaved draft");
        assert!(document.is_dirty());
        assert_eq!(document.tab_title(), before_title);
        assert_eq!(app.recent_files(), before_recent);
    }
}
