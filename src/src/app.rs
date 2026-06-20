use std::path::{Path, PathBuf};

use crate::domain::{
    Document, DocumentId, DocumentMetrics, EditorSettings, FileSnapshot, LineEnding,
    LoadedDocument, MAX_RECENT_FILES, MAX_SEARCH_RESULTS, SearchDirection, SearchResult,
    TextEncoding, collect_search_results,
};
use crate::error::AppError;

const DEFAULT_LINE: u32 = 1;
const DEFAULT_COLUMN: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorSurfaceState {
    pub document_id: Option<DocumentId>,
    pub selection_start_utf16: u32,
    pub selection_end_utf16: u32,
    pub line: u32,
    pub column: u32,
    pub can_undo: bool,
    pub can_redo: bool,
}

impl EditorSurfaceState {
    pub fn for_document(document_id: Option<DocumentId>) -> Self {
        Self {
            document_id,
            selection_start_utf16: 0,
            selection_end_utf16: 0,
            line: DEFAULT_LINE,
            column: DEFAULT_COLUMN,
            can_undo: false,
            can_redo: false,
        }
    }

    pub fn from_surface(
        document_id: Option<DocumentId>,
        selection_start_utf16: u32,
        selection_end_utf16: u32,
        line: u32,
        column: u32,
        can_undo: bool,
        can_redo: bool,
    ) -> Self {
        Self {
            document_id,
            selection_start_utf16,
            selection_end_utf16,
            line: line.max(DEFAULT_LINE),
            column: column.max(DEFAULT_COLUMN),
            can_undo,
            can_redo,
        }
    }
}

impl Default for EditorSurfaceState {
    fn default() -> Self {
        Self::for_document(None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrentEditorStatusKind {
    NoDocument,
    Saving,
    WhitespaceView,
    ReadOnly,
    Modified,
    Saved,
}

impl CurrentEditorStatusKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::NoDocument => "No file",
            Self::Saving => "Saving",
            Self::WhitespaceView => "Marks",
            Self::ReadOnly => "Read-only",
            Self::Modified => "Edited",
            Self::Saved => "Saved",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentEditorStatus {
    pub document_id: Option<DocumentId>,
    pub title: String,
    pub path: Option<PathBuf>,
    pub is_dirty: bool,
    pub can_save: bool,
    pub can_save_as: bool,
    pub can_edit: bool,
    pub is_read_only: bool,
    pub effective_read_only: bool,
    pub word_wrap: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub line: u32,
    pub column: u32,
    pub selection_start_utf16: u32,
    pub selection_end_utf16: u32,
    pub char_count: usize,
    pub encoding: TextEncoding,
    pub line_ending: LineEnding,
    pub status_kind: CurrentEditorStatusKind,
}

impl Default for CurrentEditorStatus {
    fn default() -> Self {
        Self {
            document_id: None,
            title: String::new(),
            path: None,
            is_dirty: false,
            can_save: false,
            can_save_as: false,
            can_edit: false,
            is_read_only: false,
            effective_read_only: false,
            word_wrap: false,
            can_undo: false,
            can_redo: false,
            line: DEFAULT_LINE,
            column: DEFAULT_COLUMN,
            selection_start_utf16: 0,
            selection_end_utf16: 0,
            char_count: 0,
            encoding: TextEncoding::Utf8,
            line_ending: LineEnding::Crlf,
            status_kind: CurrentEditorStatusKind::NoDocument,
        }
    }
}

#[derive(Default)]
struct RecentFiles {
    paths: Vec<PathBuf>,
}

impl RecentFiles {
    fn set(&mut self, files: Vec<PathBuf>) {
        self.paths.clear();
        for path in files.into_iter().rev() {
            self.record(path);
        }
    }

    fn as_slice(&self) -> &[PathBuf] {
        &self.paths
    }

    fn record(&mut self, path: PathBuf) {
        self.paths.retain(|candidate| candidate != &path);
        self.paths.insert(0, path);
        self.paths.truncate(MAX_RECENT_FILES);
    }

    fn remove(&mut self, path: &Path) {
        self.paths.retain(|candidate| candidate != path);
    }
}

#[derive(Default)]
struct SearchState {
    results: Vec<SearchResult>,
    active_result: Option<usize>,
}

impl SearchState {
    fn update(&mut self, content: &str, query: &str) -> &[SearchResult] {
        self.results = collect_search_results(content, query, MAX_SEARCH_RESULTS);
        self.active_result = (!self.results.is_empty()).then_some(0);
        &self.results
    }

    fn results(&self) -> &[SearchResult] {
        &self.results
    }

    fn set_active(&mut self, index: usize) -> Result<(), AppError> {
        if index >= self.results.len() {
            return Err(AppError::InvalidState("Result not found."));
        }
        self.active_result = Some(index);
        Ok(())
    }

    fn move_active(&mut self, direction: SearchDirection) -> Option<usize> {
        if self.results.is_empty() {
            self.active_result = None;
            return None;
        }

        let current = self.active_result.unwrap_or(0);
        let next = match direction {
            SearchDirection::Forward => (current + 1) % self.results.len(),
            SearchDirection::Backward if current == 0 => self.results.len() - 1,
            SearchDirection::Backward => current - 1,
        };
        self.active_result = Some(next);
        Some(next)
    }

    fn clear(&mut self) {
        self.results.clear();
        self.active_result = None;
    }
}

pub struct EditorApp {
    documents: Vec<Document>,
    current_index: Option<usize>,
    next_document_id: u64,
    next_untitled: u32,
    settings: EditorSettings,
    recent_files: RecentFiles,
    search: SearchState,
    current_surface_state: EditorSurfaceState,
    saving_document_id: Option<DocumentId>,
}

impl EditorApp {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            current_index: None,
            next_document_id: 1,
            next_untitled: 1,
            settings: EditorSettings::default(),
            recent_files: RecentFiles::default(),
            search: SearchState::default(),
            current_surface_state: EditorSurfaceState::default(),
            saving_document_id: None,
        }
    }

    pub fn set_settings(&mut self, settings: EditorSettings) {
        self.settings = settings.sanitized();
    }

    pub fn settings(&self) -> &EditorSettings {
        &self.settings
    }

    pub fn set_recent_files(&mut self, files: Vec<PathBuf>) {
        self.recent_files.set(files);
    }

    pub fn recent_files(&self) -> &[PathBuf] {
        self.recent_files.as_slice()
    }

    pub fn record_recent_file(&mut self, path: PathBuf) {
        self.recent_files.record(path);
    }

    pub fn remove_recent_file(&mut self, path: &Path) {
        self.recent_files.remove(path);
    }

    pub fn new_document(&mut self) -> DocumentId {
        let id = self.allocate_id();
        let sequence = self.next_untitled;
        self.next_untitled = self.next_untitled.saturating_add(1);
        self.documents.push(Document::new_untitled(id, sequence));
        self.current_index = self.documents.len().checked_sub(1);
        self.reset_current_surface_state();
        id
    }

    pub fn new_document_for_path(&mut self, path: PathBuf) -> DocumentId {
        let id = self.allocate_id();
        self.documents.push(Document::new_for_path(id, path));
        self.current_index = self.documents.len().checked_sub(1);
        self.clear_search_results();
        self.reset_current_surface_state();
        id
    }

    pub fn open_document(&mut self, loaded: LoadedDocument) -> DocumentId {
        let path = loaded.path.clone();
        let id = self.allocate_id();
        self.documents.push(Document::from_loaded(id, loaded));
        self.current_index = self.documents.len().checked_sub(1);
        self.record_recent_file(path);
        self.clear_search_results();
        self.reset_current_surface_state();
        id
    }

    pub(crate) fn open_document_with_metrics(
        &mut self,
        loaded: LoadedDocument,
        metrics: DocumentMetrics,
    ) -> DocumentId {
        let path = loaded.path.clone();
        let id = self.allocate_id();
        self.documents
            .push(Document::from_loaded_with_metrics(id, loaded, metrics));
        self.current_index = self.documents.len().checked_sub(1);
        self.record_recent_file(path);
        self.clear_search_results();
        self.reset_current_surface_state();
        id
    }

    pub fn replace_current_document(&mut self, loaded: LoadedDocument) -> Result<(), AppError> {
        let path = loaded.path.clone();
        let document = self
            .current_document_mut()
            .ok_or(AppError::InvalidState("No file open."))?;
        document.replace_from_loaded(loaded);
        self.record_recent_file(path);
        self.clear_search_results();
        self.reset_current_surface_state();
        Ok(())
    }

    pub(crate) fn replace_current_document_with_metrics(
        &mut self,
        loaded: LoadedDocument,
        metrics: DocumentMetrics,
    ) -> Result<(), AppError> {
        let path = loaded.path.clone();
        let document = self
            .current_document_mut()
            .ok_or(AppError::InvalidState("No file open."))?;
        document.replace_from_loaded_with_metrics(loaded, metrics);
        self.record_recent_file(path);
        self.clear_search_results();
        self.reset_current_surface_state();
        Ok(())
    }

    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    pub fn documents(&self) -> &[Document] {
        &self.documents
    }

    pub fn document_index_for_path(&self, path: &Path) -> Option<usize> {
        self.documents.iter().position(|document| {
            document
                .path()
                .is_some_and(|document_path| document_path.as_path() == path)
        })
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    pub fn set_current_index(&mut self, index: usize) -> Result<(), AppError> {
        if index >= self.documents.len() {
            return Err(AppError::InvalidState("Tab not found."));
        }
        self.current_index = Some(index);
        self.clear_search_results();
        self.reset_current_surface_state();
        Ok(())
    }

    pub fn current_document(&self) -> Option<&Document> {
        self.current_index
            .and_then(|index| self.documents.get(index))
    }

    pub fn current_document_mut(&mut self) -> Option<&mut Document> {
        self.current_index
            .and_then(|index| self.documents.get_mut(index))
    }

    pub fn update_current_content(&mut self, content: String) -> Result<(), AppError> {
        let document = self
            .current_document_mut()
            .ok_or(AppError::InvalidState("No file open."))?;
        document.set_content(content);
        Ok(())
    }

    pub(crate) fn update_current_content_with_metrics(
        &mut self,
        content: String,
        metrics: DocumentMetrics,
    ) -> Result<(), AppError> {
        let document = self
            .current_document_mut()
            .ok_or(AppError::InvalidState("No file open."))?;
        document.set_content_with_metrics(content, metrics);
        Ok(())
    }

    pub(crate) fn update_current_changed_view_content_with_metrics(
        &mut self,
        content: String,
        metrics: DocumentMetrics,
    ) -> Result<(), AppError> {
        let document = self
            .current_document_mut()
            .ok_or(AppError::InvalidState("No file open."))?;
        document.set_changed_view_content_with_metrics(content, metrics);
        Ok(())
    }

    pub fn mark_current_dirty_from_view(&mut self) -> Result<bool, AppError> {
        let document = self
            .current_document_mut()
            .ok_or(AppError::InvalidState("No file open."))?;
        Ok(document.mark_dirty_from_view())
    }

    pub fn set_current_encoding(&mut self, encoding: TextEncoding) -> Result<(), AppError> {
        let document = self
            .current_document_mut()
            .ok_or(AppError::InvalidState("No file open."))?;
        document.set_encoding(encoding);
        Ok(())
    }

    pub fn set_current_line_ending(&mut self, line_ending: LineEnding) -> Result<(), AppError> {
        let document = self
            .current_document_mut()
            .ok_or(AppError::InvalidState("No file open."))?;
        document.set_line_ending(line_ending);
        Ok(())
    }

    pub fn mark_current_saved(
        &mut self,
        path: PathBuf,
        encoding: TextEncoding,
        line_ending: LineEnding,
        snapshot: Option<FileSnapshot>,
    ) -> Result<(), AppError> {
        let id = self
            .current_document()
            .ok_or(AppError::InvalidState("No file open."))?
            .id();
        self.mark_document_saved(id, path, encoding, line_ending, snapshot)
    }

    pub fn mark_document_saved(
        &mut self,
        id: DocumentId,
        path: PathBuf,
        encoding: TextEncoding,
        line_ending: LineEnding,
        snapshot: Option<FileSnapshot>,
    ) -> Result<(), AppError> {
        let document = self
            .documents
            .iter_mut()
            .find(|document| document.id() == id)
            .ok_or(AppError::InvalidState("Saved file is closed."))?;
        document.mark_saved(path.clone(), encoding, line_ending, snapshot);
        if self.saving_document_id == Some(id) {
            self.saving_document_id = None;
        }
        self.record_recent_file(path);
        Ok(())
    }

    pub fn remove_current_document(&mut self) -> Option<Document> {
        let index = self.current_index?;
        if index >= self.documents.len() {
            self.current_index = None;
            return None;
        }

        let removed = self.documents.remove(index);
        self.current_index = if self.documents.is_empty() {
            None
        } else if index >= self.documents.len() {
            self.documents.len().checked_sub(1)
        } else {
            Some(index)
        };
        self.clear_search_results();
        self.reset_current_surface_state();
        Some(removed)
    }

    pub fn remove_other_documents(&mut self) -> bool {
        let Some(index) = self.current_index else {
            return false;
        };
        if self.documents.len() <= 1 || index >= self.documents.len() {
            return false;
        }

        let current = self.documents.remove(index);
        self.documents.clear();
        self.documents.push(current);
        self.current_index = Some(0);
        self.clear_search_results();
        self.reset_current_surface_state();
        true
    }

    pub fn remove_all_documents(&mut self) {
        self.documents.clear();
        self.current_index = None;
        self.clear_search_results();
        self.reset_current_surface_state();
    }

    pub fn update_current_editor_surface_state(&mut self, state: EditorSurfaceState) {
        let current_id = self.current_document().map(Document::id);
        self.current_surface_state = if state.document_id == current_id {
            state
        } else {
            EditorSurfaceState::for_document(current_id)
        };
    }

    pub fn set_saving_document(&mut self, document_id: Option<DocumentId>) {
        self.saving_document_id = document_id;
    }

    pub fn current_editor_status(&self) -> CurrentEditorStatus {
        let mut status = CurrentEditorStatus::default();
        self.current_editor_status_into(&mut status);
        status
    }

    pub(crate) fn current_editor_status_into(&self, status: &mut CurrentEditorStatus) {
        let current = self.current_document();
        let document_id = current.map(Document::id);
        let surface = if self.current_surface_state.document_id == document_id {
            self.current_surface_state
        } else {
            EditorSurfaceState::for_document(document_id)
        };
        let saving_current = document_id.is_some() && self.saving_document_id == document_id;
        let show_whitespace = self.settings.show_whitespace;
        let is_read_only = current.is_some_and(Document::is_read_only);
        let has_document = current.is_some();
        let can_edit = has_document && !saving_current && !show_whitespace && !is_read_only;
        let can_save = has_document && !saving_current && !show_whitespace && !is_read_only;
        let can_save_as = has_document && !saving_current;
        let is_dirty = current.is_some_and(Document::is_dirty);
        let status_kind = match current {
            None => CurrentEditorStatusKind::NoDocument,
            Some(_) if saving_current => CurrentEditorStatusKind::Saving,
            Some(_) if show_whitespace => CurrentEditorStatusKind::WhitespaceView,
            Some(_) if is_read_only => CurrentEditorStatusKind::ReadOnly,
            Some(_) if is_dirty => CurrentEditorStatusKind::Modified,
            Some(_) => CurrentEditorStatusKind::Saved,
        };

        if let Some(document) = current {
            let title = document.title();
            if status.title != title {
                status.title.clear();
                status.title.push_str(title);
            }
        } else {
            status.title.clear();
        }

        match (current.and_then(Document::path), &mut status.path) {
            (Some(path), Some(existing)) if existing.as_path() == path.as_path() => {}
            (Some(path), Some(existing)) => existing.clone_from(path),
            (Some(path), None) => status.path = Some(path.clone()),
            (None, _) => status.path = None,
        }

        status.document_id = document_id;
        status.is_dirty = is_dirty;
        status.can_save = can_save;
        status.can_save_as = can_save_as;
        status.can_edit = can_edit;
        status.is_read_only = is_read_only;
        status.effective_read_only =
            has_document && (saving_current || show_whitespace || is_read_only);
        status.word_wrap = self.settings.word_wrap;
        status.can_undo = can_edit && surface.can_undo;
        status.can_redo = can_edit && surface.can_redo;
        status.line = surface.line;
        status.column = surface.column;
        status.selection_start_utf16 = surface.selection_start_utf16;
        status.selection_end_utf16 = surface.selection_end_utf16;
        status.char_count = current.map_or(0, Document::char_count);
        status.encoding = current.map_or(TextEncoding::Utf8, Document::encoding);
        status.line_ending = current.map_or(LineEnding::Crlf, Document::line_ending);
        status.status_kind = status_kind;
    }

    pub fn move_current_tab_left(&mut self) -> bool {
        let Some(index) = self.current_index else {
            return false;
        };
        if index == 0 || index >= self.documents.len() {
            return false;
        }
        self.documents.swap(index, index - 1);
        self.current_index = Some(index - 1);
        self.clear_search_results();
        true
    }

    pub fn move_current_tab_right(&mut self) -> bool {
        let Some(index) = self.current_index else {
            return false;
        };
        if index + 1 >= self.documents.len() {
            return false;
        }
        self.documents.swap(index, index + 1);
        self.current_index = Some(index + 1);
        self.clear_search_results();
        true
    }

    pub fn dirty_indices(&self) -> Vec<usize> {
        self.documents
            .iter()
            .enumerate()
            .filter_map(|(index, document)| document.is_dirty().then_some(index))
            .collect()
    }

    pub fn update_search_results(&mut self, query: &str) -> Result<&[SearchResult], AppError> {
        let index = self
            .current_index
            .ok_or(AppError::InvalidState("No file open."))?;
        let document = self
            .documents
            .get(index)
            .ok_or(AppError::InvalidState("No file open."))?;
        Ok(self.search.update(document.content(), query))
    }

    pub fn search_results(&self) -> &[SearchResult] {
        self.search.results()
    }

    pub fn set_active_search_result(&mut self, index: usize) -> Result<(), AppError> {
        self.search.set_active(index)
    }

    pub fn move_active_search_result(&mut self, direction: SearchDirection) -> Option<usize> {
        self.search.move_active(direction)
    }

    pub fn clear_search_results(&mut self) {
        self.search.clear();
    }

    fn allocate_id(&mut self) -> DocumentId {
        let id = DocumentId::new(self.next_document_id);
        self.next_document_id = self.next_document_id.saturating_add(1);
        id
    }

    fn reset_current_surface_state(&mut self) {
        self.current_surface_state =
            EditorSurfaceState::for_document(self.current_document().map(Document::id));
    }
}

impl Default for EditorApp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FileSnapshot, LoadedDocument};

    fn loaded(path: &str) -> LoadedDocument {
        LoadedDocument {
            path: PathBuf::from(path),
            content: String::new(),
            encoding: TextEncoding::Utf8,
            line_ending: LineEnding::Crlf,
            snapshot: Some(FileSnapshot {
                modified: None,
                byte_len: 0,
            }),
            read_only_reason: None,
        }
    }

    fn loaded_with_content(path: &str, content: &str) -> LoadedDocument {
        LoadedDocument {
            content: content.to_string(),
            ..loaded(path)
        }
    }

    fn loaded_read_only(path: &str) -> LoadedDocument {
        LoadedDocument {
            read_only_reason: Some(crate::domain::ReadOnlyReason::FileAttribute),
            ..loaded(path)
        }
    }

    #[test]
    fn recent_files_are_deduplicated_newest_first() {
        let mut app = EditorApp::new();
        app.record_recent_file(PathBuf::from("a.txt"));
        app.record_recent_file(PathBuf::from("b.txt"));
        app.record_recent_file(PathBuf::from("a.txt"));

        assert_eq!(
            app.recent_files(),
            &[PathBuf::from("a.txt"), PathBuf::from("b.txt")]
        );
    }

    #[test]
    fn moving_tabs_preserves_active_document() {
        let mut app = EditorApp::new();
        app.open_document(loaded("a.txt"));
        app.open_document(loaded("b.txt"));

        assert!(app.move_current_tab_left());
        assert_eq!(app.current_index(), Some(0));
        assert_eq!(
            app.current_document().and_then(|document| document.path()),
            Some(&PathBuf::from("b.txt"))
        );
    }

    #[test]
    fn removing_other_tabs_keeps_active_document() {
        let mut app = EditorApp::new();
        app.open_document(loaded("a.txt"));
        app.open_document(loaded("b.txt"));
        app.open_document(loaded("c.txt"));
        app.set_current_index(1).expect("select middle tab");

        assert!(app.remove_other_documents());

        assert_eq!(app.document_count(), 1);
        assert_eq!(app.current_index(), Some(0));
        assert_eq!(
            app.current_document().and_then(|document| document.path()),
            Some(&PathBuf::from("b.txt"))
        );
    }

    #[test]
    fn removing_all_tabs_clears_active_document() {
        let mut app = EditorApp::new();
        app.open_document(loaded("a.txt"));
        app.open_document(loaded("b.txt"));

        app.remove_all_documents();

        assert_eq!(app.document_count(), 0);
        assert_eq!(app.current_index(), None);
    }

    #[test]
    fn document_index_for_path_finds_existing_file_tab() {
        let mut app = EditorApp::new();
        app.new_document();
        app.open_document(loaded("a.txt"));
        app.open_document(loaded("b.txt"));

        assert_eq!(app.document_index_for_path(Path::new("a.txt")), Some(1));
        assert_eq!(app.document_index_for_path(Path::new("missing.txt")), None);
    }

    #[test]
    fn dirty_indices_drive_unsaved_change_prompts() {
        let mut app = EditorApp::new();
        app.new_document();

        assert!(app.dirty_indices().is_empty());

        app.update_current_content("draft".to_string())
            .expect("edit current document");

        assert_eq!(app.dirty_indices(), vec![0]);
    }

    #[test]
    fn current_editor_status_combines_document_settings_and_surface_state() {
        let mut app = EditorApp::new();
        let document_id = app.new_document();

        app.update_current_editor_surface_state(EditorSurfaceState::from_surface(
            Some(document_id),
            3,
            7,
            2,
            4,
            true,
            false,
        ));

        let status = app.current_editor_status();
        assert_eq!(status.document_id, Some(document_id));
        assert!(!status.is_dirty);
        assert!(status.can_save);
        assert!(status.can_save_as);
        assert!(status.can_edit);
        assert!(status.can_undo);
        assert!(!status.can_redo);
        assert_eq!((status.line, status.column), (2, 4));
        assert_eq!(
            (status.selection_start_utf16, status.selection_end_utf16),
            (3, 7)
        );
        assert_eq!(status.status_kind, CurrentEditorStatusKind::Saved);

        app.update_current_content("draft".to_string())
            .expect("edit current document");
        let status = app.current_editor_status();
        assert!(status.is_dirty);
        assert_eq!(status.status_kind, CurrentEditorStatusKind::Modified);
    }

    #[test]
    fn current_editor_status_disables_mutating_commands_for_read_only_documents() {
        let mut app = EditorApp::new();
        let document_id = app.open_document(loaded_read_only("readonly.txt"));
        app.update_current_editor_surface_state(EditorSurfaceState::from_surface(
            Some(document_id),
            0,
            0,
            1,
            1,
            true,
            true,
        ));

        let status = app.current_editor_status();
        assert!(status.is_read_only);
        assert!(status.effective_read_only);
        assert!(!status.can_save);
        assert!(status.can_save_as);
        assert!(!status.can_edit);
        assert!(!status.can_undo);
        assert!(!status.can_redo);
        assert_eq!(status.status_kind, CurrentEditorStatusKind::ReadOnly);
    }

    #[test]
    fn current_editor_status_tracks_word_wrap_and_saving_state() {
        let mut app = EditorApp::new();
        let document_id = app.new_document();
        let settings = EditorSettings {
            word_wrap: true,
            show_whitespace: true,
            ..EditorSettings::default()
        };
        app.set_settings(settings);

        let status = app.current_editor_status();
        assert!(status.word_wrap);
        assert!(status.effective_read_only);
        assert!(!status.can_save);
        assert!(status.can_save_as);
        assert_eq!(status.status_kind, CurrentEditorStatusKind::WhitespaceView);

        app.set_saving_document(Some(document_id));
        let status = app.current_editor_status();
        assert!(status.effective_read_only);
        assert!(!status.can_save);
        assert!(!status.can_save_as);
        assert_eq!(status.status_kind, CurrentEditorStatusKind::Saving);
    }

    #[test]
    fn delayed_view_sync_marks_dirty_without_replacing_content() {
        let mut app = EditorApp::new();
        app.open_document(loaded_with_content("a.txt", "persisted"));
        let before = app
            .current_document()
            .expect("current document")
            .content_snapshot();

        app.mark_current_dirty_from_view()
            .expect("mark pending edit dirty");

        let document = app.current_document().expect("current document");
        assert!(document.is_dirty());
        assert_eq!(document.content(), "persisted");
        assert!(std::sync::Arc::ptr_eq(
            &before,
            &document.content_snapshot()
        ));
    }

    #[test]
    fn repeated_view_dirty_marks_only_report_first_transition() {
        let mut app = EditorApp::new();
        app.open_document(loaded_with_content("a.txt", "persisted"));

        assert!(
            app.mark_current_dirty_from_view()
                .expect("first pending edit marks dirty")
        );
        assert!(
            !app.mark_current_dirty_from_view()
                .expect("repeated pending edit stays dirty")
        );
    }

    #[test]
    fn syncing_view_text_back_to_loaded_baseline_marks_document_clean() {
        let mut app = EditorApp::new();
        app.open_document(loaded_with_content("a.txt", "persisted"));

        app.mark_current_dirty_from_view()
            .expect("mark pending edit dirty");
        app.update_current_changed_view_content_with_metrics(
            "persisted".to_string(),
            DocumentMetrics::from_char_count("persisted".chars().count()),
        )
        .expect("sync loaded baseline text");

        let document = app.current_document().expect("current document");
        assert!(!document.is_dirty());
        assert_eq!(document.content(), "persisted");
    }

    #[test]
    fn save_as_marks_document_clean_and_records_recent_file() {
        let mut app = EditorApp::new();
        app.new_document();
        app.update_current_content("draft".to_string())
            .expect("edit current document");
        let path = PathBuf::from("C:\\Temp\\saved.txt");

        app.mark_current_saved(
            path.clone(),
            TextEncoding::Utf8,
            LineEnding::Crlf,
            Some(FileSnapshot {
                modified: None,
                byte_len: 5,
            }),
        )
        .expect("mark saved");

        let document = app.current_document().expect("current document");
        assert!(!document.is_dirty());
        assert_eq!(document.path(), Some(&path));
        assert_eq!(app.recent_files(), &[path]);
    }

    #[test]
    fn new_document_for_path_does_not_add_recent_until_saved() {
        let mut app = EditorApp::new();
        let path = PathBuf::from("C:\\Temp\\future.txt");

        app.new_document_for_path(path.clone());

        let document = app.current_document().expect("current document");
        assert_eq!(document.path(), Some(&path));
        assert!(document.backing_file_missing());
        assert!(!document.is_dirty());
        assert!(app.recent_files().is_empty());
    }

    #[test]
    fn tab_switch_clears_search_results_without_mutating_documents() {
        let mut app = EditorApp::new();
        app.open_document(loaded_with_content("a.txt", "needle in first"));
        app.open_document(loaded_with_content(
            "b.txt",
            "needle in second\nneedle again",
        ));

        app.update_search_results("needle")
            .expect("search active tab");
        assert_eq!(app.search_results().len(), 2);

        app.set_current_index(0).expect("switch tab");

        assert!(app.search_results().is_empty());
        assert_eq!(
            app.current_document().map(|document| document.content()),
            Some("needle in first")
        );
    }
}
