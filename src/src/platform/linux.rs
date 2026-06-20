use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use gtk::glib::variant::ToVariant;
use gtk::prelude::*;
use gtk::{gdk, gio, glib, pango};
use gtk4 as gtk;

use crate::app::{CurrentEditorStatus, EditorApp, EditorSurfaceState};
use crate::domain::{
    DocumentId, EditorCommand, EditorCommandId, EditorSettings, FileSnapshot, KeyboardShortcut,
    LineEnding, LoadedTextAnalysis, MAX_DOCUMENT_LOAD_BYTES, MAX_FONT_SIZE_PT, MIN_FONT_SIZE_PT,
    ReadOnlyReason, SearchDirection, ShortcutKey, TextEncoding, ThemeMode,
    VISIBLE_WHITESPACE_RENDER_LIMIT_BYTES, all_commands, byte_index_to_utf16_offset,
    can_load_document_bytes, can_render_visible_whitespace_bytes, find_text,
    render_visible_whitespace_for_display, should_warn_large_file,
};
use crate::error::{AppError, FileAccessKind};
use crate::infra::{
    FileDocumentIo, MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES, SaveTargetExpectation, SavedFileSnapshot,
    UserDataStore,
};

const APP_ID: &str = "dev.j3tools.j3text";
const APP_TITLE: &str = "j3Text";
const TIMER_INTERVAL_MS: u32 = 250;
const PERSISTENCE_DEBOUNCE_TICKS: u8 = 2;
const DEFAULT_WINDOW_WIDTH: i32 = 600;
const DEFAULT_WINDOW_HEIGHT: i32 = 500;
const FIND_BAR_HEIGHT: i32 = 34;
const FIND_CONTROL_HEIGHT: i32 = 24;
const FIND_LABEL_WIDTH: i32 = 42;
const FIND_ENTRY_MIN_WIDTH: i32 = 120;
const REPLACE_LABEL_WIDTH: i32 = 58;
const FIND_NAV_BUTTON_WIDTH: i32 = 52;
const REPLACE_ONE_BUTTON_WIDTH: i32 = 68;
const REPLACE_ALL_BUTTON_WIDTH: i32 = 42;
const FIND_ALL_BUTTON_WIDTH: i32 = 70;
const FIND_CLOSE_BUTTON_WIDTH: i32 = 26;
const SEARCH_RESULTS_HEIGHT: i32 = 112;
const SEARCH_RESULTS_SIDE_MARGIN: i32 = 8;
const SEARCH_RESULTS_BOTTOM_MARGIN: i32 = 6;
const SEARCH_RESULTS_LIST_HEIGHT: i32 = SEARCH_RESULTS_HEIGHT - SEARCH_RESULTS_BOTTOM_MARGIN;
const COMMAND_PALETTE_HEIGHT: i32 = 154;
const COMMAND_PALETTE_MARGIN: i32 = 8;
const COMMAND_PALETTE_SPACING: i32 = 6;
const COMMAND_FILTER_HEIGHT: i32 = 24;
const COMMAND_LIST_HEIGHT: i32 = 108;
const TAB_TOOLTIP_WRAP_COLUMN: usize = 80;
const STATUS_HEIGHT: i32 = 24;
const STATUS_LINE_WIDTH: i32 = 140;
const STATUS_CHARS_WIDTH: i32 = 120;
const STATUS_SELECTED_WIDTH: i32 = 130;
const STATUS_ENCODING_WIDTH: i32 = 110;
const STATUS_LINE_ENDING_WIDTH: i32 = 110;
const STATUS_WRAP_WIDTH: i32 = 110;
const STATUS_SAVE_STATE_WIDTH: i32 = 130;
const LINE_NUMBER_WIDTH: i32 = 58;
const MAIN_WINDOW_STATE_QDATA_KEY: &str = "j3text-main-window-state";
const ACTION_SMOKE_ENV: &str = "J3TEXT_LINUX_ACTION_SMOKE";
const ACTION_SMOKE_REPORT_ENV: &str = "J3TEXT_LINUX_ACTION_SMOKE_REPORT";
const ACTION_SMOKE_OPEN_PATH_ENV: &str = "J3TEXT_LINUX_ACTION_SMOKE_OPEN_PATH";
const ACTION_SMOKE_SAVE_AS_PATH_ENV: &str = "J3TEXT_LINUX_ACTION_SMOKE_SAVE_AS_PATH";
const ACTION_SMOKE_NEW_WINDOW_PATH_ENV: &str = "J3TEXT_LINUX_ACTION_SMOKE_NEW_WINDOW_PATH";
const FILE_DIALOG_FILTERS: &[FileDialogFilterSpec] = &[
    FileDialogFilterSpec {
        name: "Text (*.txt)",
        pattern: "*.txt",
    },
    FileDialogFilterSpec {
        name: "All (*.*)",
        pattern: "*",
    },
];

thread_local! {
    static ACTIVE_MODAL_DIALOG_DEPTH: Cell<u32> = const { Cell::new(0) };
}

#[derive(Clone, Copy)]
struct FileDialogFilterSpec {
    name: &'static str,
    pattern: &'static str,
}

type ChoiceButtonSpec = (&'static str, gtk::ResponseType);

const YES_NO_BUTTONS: &[ChoiceButtonSpec] = &[
    ("Yes", gtk::ResponseType::Yes),
    ("No", gtk::ResponseType::No),
];
const YES_NO_CANCEL_BUTTONS: &[ChoiceButtonSpec] = &[
    ("Yes", gtk::ResponseType::Yes),
    ("No", gtk::ResponseType::No),
    ("Cancel", gtk::ResponseType::Cancel),
];
const EXTERNAL_CHANGE_RELOAD_RESPONSE: u16 = 101;
const EXTERNAL_CHANGE_SAVE_AS_RESPONSE: u16 = 102;
const EXTERNAL_FILE_CHANGED_BUTTONS: &[ChoiceButtonSpec] = &[
    (
        "Reload",
        gtk::ResponseType::Other(EXTERNAL_CHANGE_RELOAD_RESPONSE),
    ),
    (
        "Save As",
        gtk::ResponseType::Other(EXTERNAL_CHANGE_SAVE_AS_RESPONSE),
    ),
    ("Cancel", gtk::ResponseType::Cancel),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MessageDialogContent<'a> {
    window_title: &'a str,
    body_text: &'a str,
    secondary_text: Option<&'a str>,
}

const ERROR_DIALOG_TITLE: &str = "j3Text Error";
const FILE_CHANGED_DIALOG_TITLE: &str = "File Changed";
const ABOUT_DIALOG_TITLE: &str = "About";
const ABOUT_DIALOG_MESSAGE: &str = concat!("j3Text ", env!("CARGO_PKG_VERSION"));
const ABOUT_DIALOG_URL: &str = "https://github.com/edgarp9";
const FIND_DIALOG_TITLE: &str = "Find";
const RESULTS_DIALOG_TITLE: &str = "Results";
const NO_MATCH_DIALOG_MESSAGE: &str = "No match.";
const SAVING_DIALOG_TITLE: &str = "Saving";
const SAVE_STILL_RUNNING_MESSAGE: &str = "Save is still running. Please wait.";
const SAVE_AS_OVERWRITE_DIALOG_TITLE: &str = "Confirm Save As";

pub(crate) fn run() -> Result<(), AppError> {
    let startup_paths = startup_file_paths_from_args(env::args_os());
    let startup_error: Rc<RefCell<Option<AppError>>> = Rc::new(RefCell::new(None));
    let application = gtk::Application::builder().application_id(APP_ID).build();

    {
        let startup_paths = startup_paths.clone();
        let startup_error = Rc::clone(&startup_error);
        application.connect_activate(move |application| {
            match MainWindow::create(application, startup_paths.clone()) {
                Ok(window) => {
                    window.borrow().window.present();
                    MainWindow::restore_startup_state_later(&window);
                    MainWindow::start_status_timer(&window);
                    MainWindow::start_action_smoke_if_requested(&window);
                }
                Err(error) => {
                    show_error_dialog(None, &error);
                    *startup_error.borrow_mut() = Some(error);
                    application.quit();
                }
            }
        });
    }

    application.run_with_args(&[APP_TITLE]);

    if let Some(error) = startup_error.borrow_mut().take() {
        Err(error)
    } else {
        Ok(())
    }
}

fn startup_file_paths_from_args(args: impl IntoIterator<Item = OsString>) -> Vec<PathBuf> {
    args.into_iter()
        .skip(1)
        .filter(|arg| !arg.as_os_str().is_empty())
        .map(PathBuf::from)
        .collect()
}

pub(crate) fn report_fatal_startup_error(error: &AppError) {
    eprintln!("{}", error.user_message());
    if gtk::init().is_ok() {
        show_error_dialog(None, error);
    }
}

fn main_window_state_quark() -> glib::Quark {
    glib::Quark::from_str(MAIN_WINDOW_STATE_QDATA_KEY)
}

fn retain_main_window_state(window: &gtk::ApplicationWindow, state: Rc<RefCell<MainWindow>>) {
    // SAFETY: The qdata key is private to this module and always stores the same
    // Rc<RefCell<MainWindow>> type. It is stolen with the same type on close.
    unsafe {
        window.set_qdata(main_window_state_quark(), state);
    }
}

fn release_main_window_state(window: &gtk::ApplicationWindow) {
    // SAFETY: The qdata key is private to this module and only set by
    // retain_main_window_state with this exact type.
    let _ = unsafe { window.steal_qdata::<Rc<RefCell<MainWindow>>>(main_window_state_quark()) };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PendingPersistence {
    settings: bool,
    recent_files: bool,
    ticks_until_flush: u8,
}

impl PendingPersistence {
    fn request_settings(&mut self) {
        self.settings = true;
        self.defer_flush();
    }

    fn request_recent_files(&mut self) {
        self.recent_files = true;
        self.defer_flush();
    }

    fn has_pending(self) -> bool {
        self.settings || self.recent_files
    }

    fn tick_elapsed(&mut self) -> bool {
        if !self.has_pending() {
            return false;
        }
        self.ticks_until_flush = self.ticks_until_flush.saturating_sub(1);
        self.ticks_until_flush == 0
    }

    fn take_pending(&mut self) -> Self {
        let pending = *self;
        *self = Self::default();
        pending
    }

    fn defer_flush(&mut self) {
        self.ticks_until_flush = PERSISTENCE_DEBOUNCE_TICKS;
    }
}

struct PendingSave {
    document_id: crate::domain::DocumentId,
    path: PathBuf,
    encoding: TextEncoding,
    line_ending: LineEnding,
    receiver: Receiver<Result<SavedFileSnapshot, AppError>>,
}

struct PendingPlainTextPaste {
    document_id: DocumentId,
    start_mark: gtk::TextMark,
    end_mark: gtk::TextMark,
}

#[derive(Clone, Copy)]
enum SaveMode {
    Blocking {
        reload_current_document_into_edit: bool,
    },
    Background,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GtkEditorViewState {
    selection_start: i32,
    selection_end: i32,
    vadjustment: i32,
    had_selection: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineNumbersSnapshot {
    first_line: usize,
    visible_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VisibleWhitespaceDisplayCacheKey {
    document_id: crate::domain::DocumentId,
    content_generation: u64,
    source_len: usize,
    show_whitespace: bool,
}

impl VisibleWhitespaceDisplayCacheKey {
    fn new(
        document_id: crate::domain::DocumentId,
        content_generation: u64,
        source_len: usize,
        show_whitespace: bool,
    ) -> Self {
        Self {
            document_id,
            content_generation,
            source_len,
            show_whitespace,
        }
    }
}

struct VisibleWhitespaceDisplayCache {
    key: VisibleWhitespaceDisplayCacheKey,
    rendered: Option<Arc<String>>,
}

impl VisibleWhitespaceDisplayCache {
    fn new(key: VisibleWhitespaceDisplayCacheKey, rendered: Option<String>) -> Self {
        Self {
            key,
            rendered: rendered.map(Arc::new),
        }
    }

    fn matches(&self, key: VisibleWhitespaceDisplayCacheKey) -> bool {
        self.key == key
    }

    fn display_text<'a>(&self, source: &'a str) -> VisibleWhitespaceDisplayText<'a> {
        match &self.rendered {
            Some(rendered) => VisibleWhitespaceDisplayText::Rendered(Arc::clone(rendered)),
            None => VisibleWhitespaceDisplayText::Source(source),
        }
    }
}

enum VisibleWhitespaceDisplayText<'a> {
    Source(&'a str),
    Rendered(Arc<String>),
}

impl VisibleWhitespaceDisplayText<'_> {
    fn as_str(&self) -> &str {
        match self {
            Self::Source(source) => source,
            Self::Rendered(rendered) => rendered.as_str(),
        }
    }
}

struct StatusLabels {
    line: gtk::Label,
    chars: gtk::Label,
    selected: gtk::Label,
    encoding: gtk::Label,
    line_ending: gtk::Label,
    wrap: gtk::Label,
    save_state: gtk::Label,
    detail: gtk::Label,
}

struct MainWindow {
    self_weak: Weak<RefCell<MainWindow>>,
    application: gtk::Application,
    window: gtk::ApplicationWindow,
    menu_bar: gtk::PopoverMenuBar,
    notebook: gtk::Notebook,
    text_view: gtk::TextView,
    buffer: gtk::TextBuffer,
    editor_scrolled: gtk::ScrolledWindow,
    line_numbers: gtk::TextView,
    line_numbers_buffer: gtk::TextBuffer,
    line_numbers_scrolled: gtk::ScrolledWindow,
    find_bar: gtk::Box,
    find_entry: gtk::Entry,
    replace_entry: gtk::Entry,
    search_results_panel: gtk::ScrolledWindow,
    search_results_list: gtk::ListBox,
    command_palette: gtk::Box,
    command_filter: gtk::Entry,
    command_results_panel: gtk::ScrolledWindow,
    command_list: gtk::ListBox,
    status: StatusLabels,
    css_provider: gtk::CssProvider,
    actions: HashMap<&'static str, gio::SimpleAction>,
    app: EditorApp,
    io: FileDocumentIo,
    store: Option<UserDataStore>,
    startup_paths: Vec<PathBuf>,
    startup_warnings: Vec<String>,
    pending_persistence: PendingPersistence,
    programmatic_update: bool,
    edit_content_pending_sync: bool,
    pending_save: Option<PendingSave>,
    editor_view_states: HashMap<crate::domain::DocumentId, GtkEditorViewState>,
    line_numbers_snapshot: Option<LineNumbersSnapshot>,
    visible_whitespace_display_cache:
        HashMap<crate::domain::DocumentId, VisibleWhitespaceDisplayCache>,
    show_find_bar: bool,
    show_search_results: bool,
    show_command_palette: bool,
    show_line_numbers: bool,
    last_persist_error: Option<String>,
    command_items: Vec<EditorCommand>,
    filtered_command_ids: Vec<EditorCommandId>,
    pressed_editor_shortcut_keys: HashSet<ShortcutKey>,
    dirty_prompt_smoke_decision: Cell<Option<DialogChoice>>,
    yes_no_prompt_smoke_decision: Cell<Option<DialogChoice>>,
}

impl MainWindow {
    fn create(
        application: &gtk::Application,
        startup_paths: Vec<PathBuf>,
    ) -> Result<Rc<RefCell<Self>>, AppError> {
        let mut app = EditorApp::new();
        let mut startup_warnings = Vec::new();
        let store = match UserDataStore::new() {
            Ok(store) => {
                match store.load_settings() {
                    Ok(settings) => app.set_settings(settings),
                    Err(error) => startup_warnings.push(error.user_message()),
                }
                match store.load_recent_files() {
                    Ok(recent_files) => app.set_recent_files(recent_files),
                    Err(error) => startup_warnings.push(error.user_message()),
                }
                Some(store)
            }
            Err(error) => {
                startup_warnings.push(error.user_message());
                None
            }
        };

        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title(APP_TITLE)
            .default_width(DEFAULT_WINDOW_WIDTH)
            .default_height(DEFAULT_WINDOW_HEIGHT)
            .build();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window.set_child(Some(&root));

        let menu_bar = gtk::PopoverMenuBar::from_model(Some(&build_menu_model(&app)));
        root.append(&menu_bar);

        let notebook = gtk::Notebook::new();
        notebook.set_scrollable(true);
        notebook.set_show_border(false);
        notebook.set_hexpand(true);
        root.append(&notebook);

        let find_bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        find_bar.set_size_request(-1, FIND_BAR_HEIGHT);
        find_bar.set_margin_start(6);
        find_bar.set_margin_end(6);
        find_bar.set_margin_top(4);
        find_bar.set_margin_bottom(4);
        let find_label = gtk::Label::new(Some("Find"));
        find_label.set_size_request(FIND_LABEL_WIDTH, FIND_CONTROL_HEIGHT);
        let find_entry = gtk::Entry::new();
        find_entry.set_size_request(FIND_ENTRY_MIN_WIDTH, FIND_CONTROL_HEIGHT);
        find_entry.set_hexpand(true);
        let replace_label = gtk::Label::new(Some("With"));
        replace_label.set_size_request(REPLACE_LABEL_WIDTH, FIND_CONTROL_HEIGHT);
        let replace_entry = gtk::Entry::new();
        replace_entry.set_size_request(FIND_ENTRY_MIN_WIDTH, FIND_CONTROL_HEIGHT);
        replace_entry.set_hexpand(true);
        find_bar.append(&find_label);
        find_bar.append(&find_entry);
        find_bar.append(&replace_label);
        find_bar.append(&replace_entry);
        for (label, action, width) in [
            ("Next", "win.find-next", FIND_NAV_BUTTON_WIDTH),
            ("Prev", "win.find-previous", FIND_NAV_BUTTON_WIDTH),
            ("One", "win.replace-current", REPLACE_ONE_BUTTON_WIDTH),
            ("All", "win.replace-all", REPLACE_ALL_BUTTON_WIDTH),
            ("List", "win.find-all", FIND_ALL_BUTTON_WIDTH),
            ("X", "win.close-find", FIND_CLOSE_BUTTON_WIDTH),
        ] {
            let button = gtk::Button::with_label(label);
            button.set_size_request(width, FIND_CONTROL_HEIGHT);
            button.set_action_name(Some(action));
            find_bar.append(&button);
        }
        root.append(&find_bar);

        let command_palette = gtk::Box::new(gtk::Orientation::Vertical, COMMAND_PALETTE_SPACING);
        command_palette.set_size_request(-1, COMMAND_PALETTE_HEIGHT);
        command_palette.set_margin_start(COMMAND_PALETTE_MARGIN);
        command_palette.set_margin_end(COMMAND_PALETTE_MARGIN);
        command_palette.set_margin_top(COMMAND_PALETTE_MARGIN);
        command_palette.set_margin_bottom(COMMAND_PALETTE_MARGIN);
        let command_filter = gtk::Entry::new();
        command_filter.set_size_request(-1, COMMAND_FILTER_HEIGHT);
        command_filter.set_placeholder_text(Some("Command"));
        let command_scrolled = gtk::ScrolledWindow::builder()
            .min_content_height(COMMAND_LIST_HEIGHT)
            .max_content_height(COMMAND_LIST_HEIGHT)
            .vexpand(false)
            .build();
        command_scrolled.set_size_request(-1, COMMAND_LIST_HEIGHT);
        let command_list = gtk::ListBox::new();
        command_list.set_activate_on_single_click(false);
        command_scrolled.set_child(Some(&command_list));
        command_palette.append(&command_filter);
        command_palette.append(&command_scrolled);
        root.append(&command_palette);

        let editor_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        editor_row.set_hexpand(true);
        editor_row.set_vexpand(true);
        root.append(&editor_row);

        let line_numbers_buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        let line_numbers = gtk::TextView::builder()
            .buffer(&line_numbers_buffer)
            .editable(false)
            .cursor_visible(false)
            .monospace(true)
            .wrap_mode(gtk::WrapMode::None)
            .justification(gtk::Justification::Right)
            .build();
        line_numbers.add_css_class("line-numbers");
        line_numbers.set_left_margin(8);
        line_numbers.set_right_margin(8);
        line_numbers.set_top_margin(4);
        line_numbers.set_bottom_margin(4);
        let line_numbers_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .min_content_width(LINE_NUMBER_WIDTH)
            .max_content_width(LINE_NUMBER_WIDTH)
            .build();
        line_numbers_scrolled.set_child(Some(&line_numbers));
        editor_row.append(&line_numbers_scrolled);

        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        buffer.set_enable_undo(true);
        let text_view = gtk::TextView::builder()
            .buffer(&buffer)
            .monospace(true)
            .wrap_mode(if app.settings().word_wrap {
                gtk::WrapMode::WordChar
            } else {
                gtk::WrapMode::None
            })
            .accepts_tab(true)
            .build();
        text_view.add_css_class("editor");
        text_view.set_left_margin(6);
        text_view.set_right_margin(6);
        text_view.set_top_margin(4);
        text_view.set_bottom_margin(4);
        let editor_scrolled = gtk::ScrolledWindow::new();
        editor_scrolled.set_hexpand(true);
        editor_scrolled.set_vexpand(true);
        editor_scrolled.set_child(Some(&text_view));
        editor_row.append(&editor_scrolled);

        let search_results_panel = gtk::ScrolledWindow::builder()
            .min_content_height(SEARCH_RESULTS_LIST_HEIGHT)
            .max_content_height(SEARCH_RESULTS_LIST_HEIGHT)
            .vexpand(false)
            .build();
        search_results_panel.set_margin_start(SEARCH_RESULTS_SIDE_MARGIN);
        search_results_panel.set_margin_end(SEARCH_RESULTS_SIDE_MARGIN);
        search_results_panel.set_margin_bottom(SEARCH_RESULTS_BOTTOM_MARGIN);
        let search_results_list = gtk::ListBox::new();
        search_results_list.set_activate_on_single_click(false);
        search_results_panel.set_child(Some(&search_results_list));
        root.append(&search_results_panel);

        let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        status_box.set_size_request(-1, STATUS_HEIGHT);
        status_box.add_css_class("status");
        status_box.set_margin_start(8);
        status_box.set_margin_end(8);
        status_box.set_margin_top(3);
        status_box.set_margin_bottom(3);
        let status = StatusLabels {
            line: status_label(),
            chars: status_label(),
            selected: status_label(),
            encoding: status_label(),
            line_ending: status_label(),
            wrap: status_label(),
            save_state: status_label(),
            detail: gtk::Label::new(None),
        };
        status.line.set_width_request(STATUS_LINE_WIDTH);
        status.chars.set_width_request(STATUS_CHARS_WIDTH);
        status.selected.set_width_request(STATUS_SELECTED_WIDTH);
        status.encoding.set_width_request(STATUS_ENCODING_WIDTH);
        status
            .line_ending
            .set_width_request(STATUS_LINE_ENDING_WIDTH);
        status.wrap.set_width_request(STATUS_WRAP_WIDTH);
        status.save_state.set_width_request(STATUS_SAVE_STATE_WIDTH);
        status.detail.set_hexpand(true);
        status.detail.set_xalign(0.0);
        for label in [
            &status.line,
            &status.chars,
            &status.selected,
            &status.encoding,
            &status.line_ending,
            &status.wrap,
            &status.save_state,
            &status.detail,
        ] {
            status_box.append(label);
        }
        root.append(&status_box);

        let css_provider = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &css_provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let window_state = Rc::new(RefCell::new(Self {
            self_weak: Weak::new(),
            application: application.clone(),
            window,
            menu_bar,
            notebook,
            text_view,
            buffer,
            editor_scrolled,
            line_numbers,
            line_numbers_buffer,
            line_numbers_scrolled,
            find_bar,
            find_entry,
            replace_entry,
            search_results_panel,
            search_results_list,
            command_palette,
            command_filter,
            command_results_panel: command_scrolled,
            command_list,
            status,
            css_provider,
            actions: HashMap::new(),
            app,
            io: FileDocumentIo::new(),
            store,
            startup_paths,
            startup_warnings,
            pending_persistence: PendingPersistence::default(),
            programmatic_update: false,
            edit_content_pending_sync: false,
            pending_save: None,
            editor_view_states: HashMap::new(),
            line_numbers_snapshot: None,
            visible_whitespace_display_cache: HashMap::new(),
            show_find_bar: false,
            show_search_results: false,
            show_command_palette: false,
            show_line_numbers: true,
            last_persist_error: None,
            command_items: all_commands(),
            filtered_command_ids: Vec::new(),
            pressed_editor_shortcut_keys: HashSet::new(),
            dirty_prompt_smoke_decision: Cell::new(None),
            yes_no_prompt_smoke_decision: Cell::new(None),
        }));
        window_state.borrow_mut().self_weak = Rc::downgrade(&window_state);

        Self::install_actions(&window_state);
        Self::connect_signals(&window_state);
        {
            let mut state = window_state.borrow_mut();
            state.apply_settings_to_view();
            state.refresh_tabs();
            state.load_current_document_into_edit()?;
            state.show_or_hide_find_bar();
            state.show_or_hide_search_results();
            state.show_or_hide_command_palette();
            state.update_command_palette_filter()?;
            state.update_status();
            state.update_menu_checks();
            state.show_startup_warnings();
        }

        retain_main_window_state(&window_state.borrow().window, Rc::clone(&window_state));

        Ok(window_state)
    }

    fn install_actions(this: &Rc<RefCell<Self>>) {
        let initial_settings = this.borrow().app.settings().clone();
        let initial_line_numbers = this.borrow().show_line_numbers;
        add_window_action(this, "new", |window| window.new_tab());
        add_window_action(this, "open", |window| window.open_file(None));
        add_window_action(this, "save", |window| window.save_current_command(false));
        add_window_action(this, "save-as", |window| window.save_current_command(true));
        add_window_action(this, "close-tab", |window| window.close_current_tab());
        add_window_action(this, "close-other-tabs", |window| window.close_other_tabs());
        add_window_action(this, "close-all-tabs", |window| window.close_all_tabs());
        add_window_action(this, "exit", |window| {
            let gtk_window = window.window.clone();
            glib::idle_add_local_once(move || {
                gtk_window.close();
            });
            Ok(())
        });
        for index in 0..crate::domain::MAX_RECENT_FILES {
            let action_name = Box::leak(format!("recent-{index}").into_boxed_str());
            add_window_action(this, action_name, move |window| {
                window.open_recent_file(index)
            });
        }
        add_window_action(this, "undo", |window| window.undo());
        add_window_action(this, "redo", |window| window.redo());
        add_window_action(this, "cut", |window| window.cut());
        add_window_action(this, "copy", |window| {
            window.copy();
            Ok(())
        });
        add_window_action(this, "paste", |window| window.paste());
        add_window_action(this, "select-all", |window| {
            window.select_all_text();
            Ok(())
        });
        add_window_action(this, "find", |window| {
            window.show_find_bar_and_focus(false);
            Ok(())
        });
        add_window_action(this, "replace", |window| {
            window.show_find_bar_and_focus(true);
            Ok(())
        });
        add_window_action(this, "find-next", |window| {
            window.search(SearchDirection::Forward)
        });
        add_window_action(this, "find-previous", |window| {
            window.search(SearchDirection::Backward)
        });
        add_window_action(this, "find-all", |window| window.find_all_results());
        add_window_action(this, "replace-current", |window| window.replace_current());
        add_window_action(this, "replace-all", |window| window.replace_all());
        add_window_action(this, "close-find", |window| {
            window.close_find_bar_and_results();
            Ok(())
        });
        add_window_toggle_action(this, "command-palette", false, |window| {
            window.toggle_command_palette()
        });
        add_window_toggle_action(this, "line-numbers", initial_line_numbers, |window| {
            window.show_line_numbers = !window.show_line_numbers;
            window.apply_line_numbers_visibility();
            window.update_line_numbers();
            window.update_menu_checks();
            Ok(())
        });
        add_window_toggle_action(
            this,
            "visible-whitespace",
            initial_settings.show_whitespace,
            |window| window.toggle_visible_whitespace(),
        );
        add_window_toggle_action(this, "word-wrap", initial_settings.word_wrap, |window| {
            window.toggle_word_wrap()
        });
        add_window_action(this, "reopen-encoding", |window| {
            window.reopen_current_file_with_encoding_dialog()
        });
        add_window_action(this, "change-encoding", |window| {
            window.convert_current_encoding_dialog()
        });
        add_window_toggle_action(this, "line-ending-crlf", false, |window| {
            window.set_line_ending(LineEnding::Crlf)
        });
        add_window_toggle_action(this, "line-ending-lf", false, |window| {
            window.set_line_ending(LineEnding::Lf)
        });
        add_window_toggle_action(this, "line-ending-cr", false, |window| {
            window.set_line_ending(LineEnding::Cr)
        });
        add_window_action(this, "tab-left", |window| window.move_current_tab_left());
        add_window_action(this, "tab-right", |window| window.move_current_tab_right());
        add_window_action(this, "open-new-window", |window| {
            window.open_current_tab_in_new_window()
        });
        add_window_action(this, "choose-font", |window| window.choose_font());
        add_window_toggle_action(
            this,
            "tab-size-2",
            initial_settings.tab_size == 2,
            |window| window.set_tab_size(2),
        );
        add_window_toggle_action(
            this,
            "tab-size-4",
            initial_settings.tab_size == 4,
            |window| window.set_tab_size(4),
        );
        add_window_toggle_action(
            this,
            "tab-size-8",
            initial_settings.tab_size == 8,
            |window| window.set_tab_size(8),
        );
        add_window_action(this, "about", |window| {
            window.show_about();
            Ok(())
        });

        for theme in ThemeMode::options() {
            let action_name = theme_action_name(*theme);
            let theme = *theme;
            add_window_toggle_action(
                this,
                action_name,
                initial_settings.theme == theme,
                move |window| window.set_theme(theme),
            );
        }

        for (index, command) in EditorCommandId::SHORTCUT_COMMANDS
            .iter()
            .copied()
            .enumerate()
        {
            let capture = Box::leak(format!("shortcut-{index}-capture").into_boxed_str());
            let default = Box::leak(format!("shortcut-{index}-default").into_boxed_str());
            let disable = Box::leak(format!("shortcut-{index}-disable").into_boxed_str());
            add_window_action(this, capture, move |window| {
                window.configure_shortcut(command, ShortcutMenuAction::Capture)
            });
            add_window_toggle_action(
                this,
                default,
                initial_settings.shortcuts.shortcut_for(command) == command.default_shortcut(),
                move |window| window.configure_shortcut(command, ShortcutMenuAction::UseDefault),
            );
            add_window_toggle_action(
                this,
                disable,
                initial_settings.shortcuts.shortcut_for(command).is_none(),
                move |window| window.configure_shortcut(command, ShortcutMenuAction::Disable),
            );
        }

        this.borrow().update_accelerators();
    }

    fn connect_signals(this: &Rc<RefCell<Self>>) {
        if let Some(settings) = gtk::Settings::default() {
            let weak = Rc::downgrade(this);
            settings.connect_gtk_application_prefer_dark_theme_notify(move |_| {
                if let Some(this) = weak.upgrade()
                    && let Ok(window) = this.try_borrow()
                {
                    window.apply_system_theme_change();
                }
            });
            let weak = Rc::downgrade(this);
            settings.connect_gtk_theme_name_notify(move |_| {
                if let Some(this) = weak.upgrade()
                    && let Ok(window) = this.try_borrow()
                {
                    window.apply_system_theme_change();
                }
            });
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow().buffer.connect_changed(move |_| {
                with_window_report_errors(&weak, |window| window.on_text_changed());
            });
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow().buffer.connect_paste_done(move |_, _| {
                with_window_report_errors(&weak, |window| {
                    if window.programmatic_update {
                        return Ok(());
                    }
                    window.edit_content_pending_sync = true;
                    window.sync_current_text()?;
                    window.update_status();
                    window.update_line_numbers();
                    window.update_menu_checks();
                    Ok(())
                });
            });
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow().buffer.connect_mark_set(move |_, _, _| {
                if let Some(this) = weak.upgrade()
                    && let Ok(mut window) = this.try_borrow_mut()
                {
                    window.on_editor_surface_view_changed();
                }
            });
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow().buffer.connect_can_undo_notify(move |_| {
                if let Some(this) = weak.upgrade()
                    && let Ok(mut window) = this.try_borrow_mut()
                {
                    window.on_editor_surface_view_changed();
                }
            });
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow().buffer.connect_can_redo_notify(move |_| {
                if let Some(this) = weak.upgrade()
                    && let Ok(mut window) = this.try_borrow_mut()
                {
                    window.on_editor_surface_view_changed();
                }
            });
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow()
                .editor_scrolled
                .vadjustment()
                .connect_value_changed(move |_| {
                    if let Some(this) = weak.upgrade()
                        && let Ok(mut window) = this.try_borrow_mut()
                    {
                        window.on_editor_surface_view_changed();
                    }
                });
        }
        {
            let key_controller = gtk::EventControllerKey::new();
            key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
            let weak = Rc::downgrade(this);
            key_controller.connect_key_pressed(move |_, key, _, state| {
                let Some(shortcut) = shortcut_from_key_event(key, state) else {
                    return glib::Propagation::Proceed;
                };
                let Some(this) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let result = {
                    let Ok(mut window) = this.try_borrow_mut() else {
                        return glib::Propagation::Proceed;
                    };
                    let command_id = window.app.settings().shortcuts.command_for(shortcut);
                    let Some(command_id) =
                        command_id.filter(|command| editor_control_shortcut_command(*command))
                    else {
                        return glib::Propagation::Proceed;
                    };
                    if !window.pressed_editor_shortcut_keys.insert(shortcut.key) {
                        return glib::Propagation::Stop;
                    }
                    window.execute_editor_command(command_id)
                };
                if let Err(error) = result {
                    if let Ok(window) = this.try_borrow() {
                        show_error_dialog(Some(&window.window), &error);
                    } else {
                        show_error_dialog(None, &error);
                    }
                }
                glib::Propagation::Stop
            });
            let weak = Rc::downgrade(this);
            key_controller.connect_key_released(move |_, key, _, _| {
                let Some(shortcut_key) = shortcut_key_from_gdk_key(key) else {
                    return;
                };
                let Some(this) = weak.upgrade() else {
                    return;
                };
                if let Ok(mut window) = this.try_borrow_mut() {
                    window.pressed_editor_shortcut_keys.remove(&shortcut_key);
                }
            });
            this.borrow().text_view.add_controller(key_controller);
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow()
                .notebook
                .connect_switch_page(move |_, _, page| {
                    with_window_report_errors(&weak, |window| window.select_tab(page as usize));
                });
        }
        {
            let gesture = gtk::GestureClick::new();
            gesture.set_button(3);
            let weak = Rc::downgrade(this);
            gesture.connect_pressed(move |_, _, x, y| {
                with_window_report_errors(&weak, |window| window.show_tab_context_menu(x, y));
            });
            this.borrow().notebook.add_controller(gesture);
        }
        {
            let gesture = gtk::GestureClick::new();
            gesture.set_button(3);
            let weak = Rc::downgrade(this);
            gesture.connect_pressed(move |_, _, x, y| {
                with_window_report_errors(&weak, |window| {
                    window.show_editor_context_menu(x, y);
                    Ok(())
                });
            });
            this.borrow().text_view.add_controller(gesture);
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow().find_entry.connect_changed(move |_| {
                if let Some(this) = weak.upgrade()
                    && let Ok(mut window) = this.try_borrow_mut()
                {
                    window.on_find_query_changed();
                }
            });
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow().find_entry.connect_activate(move |_| {
                with_window_report_errors(&weak, |window| {
                    window.search_from_find_entry(SearchDirection::Forward)
                });
            });
        }
        for entry in [
            this.borrow().find_entry.clone(),
            this.borrow().replace_entry.clone(),
        ] {
            let key_controller = gtk::EventControllerKey::new();
            let weak = Rc::downgrade(this);
            key_controller.connect_key_pressed(move |_, key, _, _| {
                if key == gdk::Key::Escape {
                    if let Some(this) = weak.upgrade()
                        && let Ok(mut window) = this.try_borrow_mut()
                    {
                        window.close_find_bar_and_results();
                    }
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
            entry.add_controller(key_controller);
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow()
                .search_results_list
                .connect_row_activated(move |_, row| {
                    with_window_report_errors(&weak, |window| {
                        window.select_search_result(row.index() as usize)
                    });
                });
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow().command_filter.connect_changed(move |_| {
                with_window_report_errors(&weak, |window| window.update_command_palette_filter());
            });
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow().command_filter.connect_activate(move |_| {
                with_window_report_errors(&weak, |window| window.activate_selected_command());
            });
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow()
                .command_list
                .connect_row_activated(move |_, row| {
                    with_window_report_errors(&weak, |window| {
                        window.activate_command_row(row.index() as usize)
                    });
                });
        }
        {
            let weak = Rc::downgrade(this);
            this.borrow().window.connect_close_request(move |_| {
                let Some(this) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let gtk_window = this.borrow().window.clone();
                let should_close = {
                    let mut window = this.borrow_mut();
                    if window.confirm_all_dirty_before_exit() {
                        window.flush_pending_persistence_report_errors();
                        true
                    } else {
                        false
                    }
                };
                if should_close {
                    release_main_window_state(&gtk_window);
                    glib::Propagation::Proceed
                } else {
                    glib::Propagation::Stop
                }
            });
        }
        {
            let weak = Rc::downgrade(this);
            let drop_target =
                gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
            drop_target.connect_drop(move |_, value, _, _| {
                let Ok(file_list) = value.get::<gdk::FileList>() else {
                    return false;
                };
                let paths = file_list
                    .files()
                    .into_iter()
                    .filter_map(|file| file.path())
                    .collect::<Vec<_>>();
                let Some(this) = weak.upgrade() else {
                    return false;
                };
                let Ok(mut window) = this.try_borrow_mut() else {
                    return false;
                };
                let parent = window.window.clone();
                let result = window.handle_drop_paths(paths);
                drop(window);
                if let Err(error) = result {
                    show_error_dialog(Some(&parent), &error);
                }
                true
            });
            this.borrow().window.add_controller(drop_target);
        }
    }

    fn restore_startup_state_later(this: &Rc<RefCell<Self>>) {
        let weak = Rc::downgrade(this);
        glib::idle_add_local_once(move || {
            with_window_report_errors(&weak, |window| window.restore_startup_state());
        });
    }

    fn start_status_timer(this: &Rc<RefCell<Self>>) {
        let weak = Rc::downgrade(this);
        glib::timeout_add_local(
            std::time::Duration::from_millis(u64::from(TIMER_INTERVAL_MS)),
            move || {
                let Some(this) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if let Ok(mut window) = this.try_borrow_mut() {
                    window.handle_status_timer();
                }
                glib::ControlFlow::Continue
            },
        );
    }

    fn start_action_smoke_if_requested(this: &Rc<RefCell<Self>>) {
        let Some(mode) = env::var_os(ACTION_SMOKE_ENV) else {
            return;
        };
        let restore_only = mode.to_string_lossy() == "restore";
        let weak = Rc::downgrade(this);
        glib::idle_add_local_once(move || {
            let weak = weak.clone();
            glib::idle_add_local_once(move || {
                let result = weak
                    .upgrade()
                    .ok_or_else(|| "window state was released".to_string())
                    .and_then(|this| {
                        if restore_only {
                            MainWindow::run_restore_smoke(&this)
                        } else {
                            MainWindow::run_action_smoke(&this)
                        }
                    });
                write_action_smoke_report(
                    if restore_only {
                        "Linux restore smoke"
                    } else {
                        "Linux action smoke"
                    },
                    result.as_ref(),
                );
                if let Some(this) = weak.upgrade()
                    && let Ok(window) = this.try_borrow()
                {
                    window.application.quit();
                }
            });
        });
    }

    fn run_restore_smoke(this: &Rc<RefCell<Self>>) -> Result<Vec<String>, String> {
        let mut steps = Vec::new();

        smoke_require(this, "restored tab size setting", |window| {
            window.app.settings().tab_size == 8
        })?;
        smoke_require_action_state(this, "tab-size-8", true, &mut steps)?;

        smoke_require(this, "restored light theme setting", |window| {
            window.app.settings().theme == ThemeMode::Light
        })?;
        smoke_require_action_state(this, "theme-light", true, &mut steps)?;

        smoke_require(this, "restored shortcut capture setting", |window| {
            window
                .app
                .settings()
                .shortcuts
                .shortcut_for(EditorCommandId::CloseTab)
                == Some(KeyboardShortcut::CTRL_F4)
        })?;
        steps.push("shortcut-close-tab-ctrl-f4".to_string());

        Ok(steps)
    }

    fn run_action_smoke(this: &Rc<RefCell<Self>>) -> Result<Vec<String>, String> {
        let mut steps = Vec::new();
        let smoke_open_path = smoke_env_path(ACTION_SMOKE_OPEN_PATH_ENV)?;
        let smoke_save_as_path = smoke_env_path(ACTION_SMOKE_SAVE_AS_PATH_ENV)?;
        let smoke_new_window_path = smoke_env_path(ACTION_SMOKE_NEW_WINDOW_PATH_ENV)?;

        smoke_require(this, "startup document exists", |window| {
            window.app.current_document().is_some()
        })?;
        smoke_require_layout_contract(this, &mut steps)?;

        let recent_count_before = smoke_read(this, |window| window.app.document_count())?;
        smoke_require(
            this,
            "startup file is available as a recent file",
            |window| !window.app.recent_files().is_empty(),
        )?;
        for (name, enabled) in [
            ("save", true),
            ("save-as", true),
            ("close-tab", true),
            ("close-other-tabs", true),
            ("close-all-tabs", true),
            ("undo", false),
            ("redo", false),
            ("cut", true),
            ("copy", true),
            ("paste", true),
            ("select-all", true),
            ("find", true),
            ("replace", true),
            ("replace-current", true),
            ("replace-all", true),
            ("reopen-encoding", true),
            ("change-encoding", true),
            ("line-ending-crlf", true),
            ("line-ending-lf", true),
            ("line-ending-cr", true),
            ("tab-left", true),
            ("tab-right", true),
        ] {
            smoke_require_action_enabled(this, name, enabled)?;
        }
        smoke_require_action_accels(this, "win.new", &["<Control>n"], &mut steps)?;
        smoke_require_action_accels(this, "win.copy", &[], &mut steps)?;
        smoke_require(
            this,
            "open new window starts disabled with one tab",
            |window| {
                window
                    .actions
                    .get("open-new-window")
                    .is_some_and(|action| !action.is_enabled())
            },
        )?;
        smoke_activate_action(this, "recent-0", &mut steps)?;
        smoke_require(this, "recent file action opens a tab", |window| {
            window.app.document_count() == recent_count_before + 1
        })?;
        smoke_require(
            this,
            "open new window enables with multiple tabs",
            |window| {
                window
                    .actions
                    .get("open-new-window")
                    .is_some_and(|action| action.is_enabled())
            },
        )?;
        let expected_new_window_path = smoke_read(this, |window| {
            window
                .app
                .current_document()
                .and_then(|document| document.path())
                .map(|path| path.display().to_string())
        })?
        .ok_or_else(|| "open-new-window smoke document had no path".to_string())?;
        smoke_activate_action(this, "open-new-window", &mut steps)?;
        let launched_path = fs::read_to_string(&smoke_new_window_path)
            .map_err(|error| format!("new-window launch path report was not readable: {error}"))?;
        if launched_path != expected_new_window_path {
            return Err(format!(
                "open-new-window launched {launched_path}, expected {expected_new_window_path}",
            ));
        }
        smoke_require(this, "open new window closes the moved tab", |window| {
            window.app.document_count() == recent_count_before
        })?;

        let missing_recent_path = smoke_open_path.with_file_name("missing-recent-action-smoke.txt");
        let _ = fs::remove_file(&missing_recent_path);
        let missing_recent_count_before = smoke_read(this, |window| window.app.document_count())?;
        smoke_set_recent_files(
            this,
            vec![missing_recent_path.clone(), smoke_open_path.clone()],
        )?;
        smoke_activate_action(this, "recent-0", &mut steps)?;
        smoke_require(this, "missing recent action removes stale path", |window| {
            window.app.document_count() == missing_recent_count_before
                && !window.app.recent_files().contains(&missing_recent_path)
                && window.app.recent_files().contains(&smoke_open_path)
        })?;

        let open_dialog_count_before = smoke_read(this, |window| window.app.document_count())?;
        smoke_activate_action(this, "open", &mut steps)?;
        smoke_require(this, "open dialog accept opens selected file", |window| {
            window.app.document_count() == open_dialog_count_before + 1
                && window
                    .app
                    .current_document()
                    .and_then(|document| document.path())
                    .is_some_and(|path| path == smoke_open_path.as_path())
        })?;

        smoke_activate_action(this, "choose-font", &mut steps)?;
        smoke_require(this, "font dialog accept applies font setting", |window| {
            let settings = window.app.settings();
            settings.font_name == "Monospace" && settings.font_size_pt == 13
        })?;

        smoke_activate_action(this, "reopen-encoding", &mut steps)?;
        smoke_require(
            this,
            "reopen encoding accept reloads current file",
            |window| {
                window.app.current_document().is_some_and(|document| {
                    document.path() == Some(&smoke_open_path)
                        && document.encoding() == TextEncoding::Utf8
                })
            },
        )?;
        smoke_activate_action(this, "change-encoding", &mut steps)?;
        smoke_require(
            this,
            "change encoding accept updates document encoding",
            |window| {
                window
                    .app
                    .current_document()
                    .is_some_and(|document| document.encoding() == TextEncoding::Utf8Bom)
            },
        )?;
        fs::write(&smoke_save_as_path, b"existing save-as target\n")
            .map_err(|error| format!("save-as overwrite seed was not writable: {error}"))?;
        smoke_set_yes_no_prompt_decision(this, DialogChoice::No)?;
        smoke_activate_action(this, "save-as", &mut steps)?;
        smoke_require(
            this,
            "save-as overwrite No keeps current target",
            |window| {
                window.app.current_document().is_some_and(|document| {
                    document.path() == Some(&smoke_open_path)
                        && document.path() != Some(&smoke_save_as_path)
                })
            },
        )?;
        let existing = fs::read_to_string(&smoke_save_as_path)
            .map_err(|error| format!("save-as overwrite seed was not readable: {error}"))?;
        if existing != "existing save-as target\n" {
            return Err("save-as overwrite No changed the selected target".to_string());
        }
        smoke_set_yes_no_prompt_decision(this, DialogChoice::Yes)?;
        smoke_activate_action(this, "save-as", &mut steps)?;
        smoke_wait_for_pending_save(this)?;
        smoke_require(this, "save-as accept saves selected target", |window| {
            window.app.current_document().is_some_and(|document| {
                document.path() == Some(&smoke_save_as_path)
                    && document.encoding() == TextEncoding::Utf8Bom
                    && !document.is_dirty()
            })
        })?;
        smoke_require_action_enabled(this, "save", true)?;
        smoke_require_action_enabled(this, "save-as", true)?;
        let saved = fs::read(&smoke_save_as_path)
            .map_err(|error| format!("save-as target was not readable: {error}"))?;
        if !saved.starts_with(&[0xEF, 0xBB, 0xBF]) {
            return Err("save-as target did not use UTF-8 BOM encoding".to_string());
        }

        smoke_insert_text(this, "\nlinux save action edit")?;
        smoke_require_action_enabled(this, "save", true)?;
        smoke_activate_action(this, "save", &mut steps)?;
        smoke_require_action_enabled(this, "save", false)?;
        smoke_wait_for_pending_save(this)?;
        smoke_require_action_enabled(this, "save", true)?;
        let saved = fs::read_to_string(&smoke_save_as_path)
            .map_err(|error| format!("save target was not readable as UTF-8: {error}"))?;
        if !saved.contains("linux save action edit") {
            return Err("save action did not write edited content".to_string());
        }

        let dirty_prompt_count = smoke_read(this, |window| window.app.document_count())?;
        smoke_insert_text(this, "\ndirty prompt cancel edit")?;
        smoke_set_dirty_prompt_decision(this, DialogChoice::Cancel)?;
        smoke_activate_action(this, "close-tab", &mut steps)?;
        smoke_require(
            this,
            "dirty prompt cancel keeps current tab dirty",
            |window| {
                window.app.document_count() == dirty_prompt_count
                    && window.app.current_document().is_some_and(|document| {
                        document.path() == Some(&smoke_save_as_path) && document.is_dirty()
                    })
                    && window.buffer_text().contains("dirty prompt cancel edit")
            },
        )?;

        smoke_set_dirty_prompt_decision(this, DialogChoice::Yes)?;
        smoke_activate_action(this, "close-tab", &mut steps)?;
        smoke_require(this, "dirty prompt save closes saved tab", |window| {
            window.app.document_count() == dirty_prompt_count.saturating_sub(1).max(1)
                && window
                    .app
                    .current_document()
                    .is_some_and(|document| document.path() != Some(&smoke_save_as_path))
        })?;
        let saved = fs::read_to_string(&smoke_save_as_path)
            .map_err(|error| format!("dirty prompt save target was not readable: {error}"))?;
        if !saved.contains("dirty prompt cancel edit") {
            return Err("dirty prompt save did not write dirty content".to_string());
        }

        let discard_base_count = smoke_read(this, |window| window.app.document_count())?;
        smoke_activate_action(this, "new", &mut steps)?;
        smoke_insert_text(this, "dirty prompt discard edit")?;
        smoke_set_dirty_prompt_decision(this, DialogChoice::No)?;
        smoke_activate_action(this, "close-tab", &mut steps)?;
        smoke_require(
            this,
            "dirty prompt discard closes current tab without keeping text",
            |window| {
                window.app.document_count() == discard_base_count
                    && !window.buffer_text().contains("dirty prompt discard edit")
            },
        )?;

        let dirty_exit_count = smoke_read(this, |window| window.app.document_count())?;
        let dirty_exit_path = smoke_read(this, |window| {
            window
                .app
                .current_document()
                .and_then(|document| document.path().cloned())
        })?
        .ok_or_else(|| "dirty exit smoke current document had no path".to_string())?;
        smoke_insert_text(this, "\ndirty exit cancel edit")?;
        smoke_set_dirty_prompt_decision(this, DialogChoice::Cancel)?;
        let confirmed = smoke_confirm_all_dirty_before_exit(this, &mut steps)?;
        if confirmed {
            return Err("dirty exit cancel unexpectedly confirmed exit".to_string());
        }
        smoke_require(
            this,
            "dirty exit cancel keeps current tab dirty",
            |window| {
                window.app.document_count() == dirty_exit_count
                    && window.app.current_document().is_some_and(|document| {
                        document.path() == Some(&dirty_exit_path) && document.is_dirty()
                    })
                    && window.buffer_text().contains("dirty exit cancel edit")
            },
        )?;
        smoke_set_dirty_prompt_decision(this, DialogChoice::No)?;
        let confirmed = smoke_confirm_all_dirty_before_exit(this, &mut steps)?;
        if !confirmed {
            return Err("dirty exit discard did not confirm exit".to_string());
        }
        smoke_require(
            this,
            "dirty exit discard keeps dirty tab for smoke",
            |window| {
                window.app.document_count() == dirty_exit_count
                    && window
                        .app
                        .current_document()
                        .is_some_and(|document| document.path() == Some(&dirty_exit_path))
                    && window.buffer_text().contains("dirty exit cancel edit")
            },
        )?;
        smoke_set_dirty_prompt_decision(this, DialogChoice::Yes)?;
        let confirmed = smoke_confirm_all_dirty_before_exit(this, &mut steps)?;
        if !confirmed {
            return Err("dirty exit save did not confirm exit".to_string());
        }
        smoke_require(this, "dirty exit save cleans current tab", |window| {
            window.app.document_count() == dirty_exit_count
                && window.app.current_document().is_some_and(|document| {
                    document.path() == Some(&dirty_exit_path) && !document.is_dirty()
                })
        })?;
        let saved = fs::read_to_string(&dirty_exit_path)
            .map_err(|error| format!("dirty exit save target was not readable: {error}"))?;
        if !saved.contains("dirty exit cancel edit") {
            return Err("dirty exit save did not write dirty content".to_string());
        }

        let dirty_open_count = smoke_read(this, |window| window.app.document_count())?;
        let dirty_open_path = smoke_read(this, |window| {
            window
                .app
                .current_document()
                .and_then(|document| document.path().cloned())
        })?
        .ok_or_else(|| "dirty open smoke current document had no path".to_string())?;
        smoke_insert_text(this, "\ndirty open cancel edit")?;
        smoke_set_dirty_prompt_decision(this, DialogChoice::Cancel)?;
        smoke_activate_action(this, "open", &mut steps)?;
        smoke_require(
            this,
            "dirty prompt cancel keeps current tab before open",
            |window| {
                window.app.document_count() == dirty_open_count
                    && window.app.current_document().is_some_and(|document| {
                        document.path() == Some(&dirty_open_path) && document.is_dirty()
                    })
                    && window.buffer_text().contains("dirty open cancel edit")
            },
        )?;

        smoke_set_dirty_prompt_decision(this, DialogChoice::No)?;
        smoke_activate_action(this, "open", &mut steps)?;
        smoke_require(
            this,
            "dirty prompt discard allows open dialog result",
            |window| {
                window.app.document_count() == dirty_open_count + 1
                    && window
                        .app
                        .current_document()
                        .and_then(|document| document.path())
                        .is_some_and(|path| path == smoke_open_path.as_path())
            },
        )?;
        smoke_set_dirty_prompt_decision(this, DialogChoice::No)?;
        smoke_activate_action(this, "close-other-tabs", &mut steps)?;
        smoke_require(this, "dirty open cleanup keeps opened tab", |window| {
            window.app.document_count() == 1
                && window
                    .app
                    .current_document()
                    .and_then(|document| document.path())
                    .is_some_and(|path| path == smoke_open_path.as_path())
        })?;

        smoke_insert_text(this, "\ndirty reopen cancel edit")?;
        smoke_set_dirty_prompt_decision(this, DialogChoice::Cancel)?;
        smoke_activate_action(this, "reopen-encoding", &mut steps)?;
        smoke_require(
            this,
            "dirty prompt cancel keeps current tab before reopen",
            |window| {
                window.app.document_count() == 1
                    && window.app.current_document().is_some_and(|document| {
                        document.path() == Some(&smoke_open_path) && document.is_dirty()
                    })
                    && window.buffer_text().contains("dirty reopen cancel edit")
            },
        )?;
        smoke_set_dirty_prompt_decision(this, DialogChoice::No)?;
        smoke_activate_action(this, "reopen-encoding", &mut steps)?;
        smoke_require(
            this,
            "dirty prompt discard allows reopen result",
            |window| {
                window.app.document_count() == 1
                    && window.app.current_document().is_some_and(|document| {
                        document.path() == Some(&smoke_open_path)
                            && document.encoding() == TextEncoding::Utf8
                            && !document.is_dirty()
                    })
                    && !window.buffer_text().contains("dirty reopen cancel edit")
            },
        )?;

        smoke_activate_action(this, "command-palette", &mut steps)?;
        smoke_require(this, "command palette opens", |window| {
            window.show_command_palette
        })?;
        smoke_require_action_state(this, "command-palette", true, &mut steps)?;
        smoke_activate_action(this, "command-palette", &mut steps)?;
        smoke_require(this, "command palette closes", |window| {
            !window.show_command_palette
        })?;
        smoke_require_action_state(this, "command-palette", false, &mut steps)?;

        let palette_line_numbers_before = smoke_read(this, |window| window.show_line_numbers)?;
        smoke_activate_action(this, "command-palette", &mut steps)?;
        smoke_require(
            this,
            "command palette opens for command execution",
            |window| window.show_command_palette,
        )?;
        smoke_activate_command_palette_filter(this, "Line Numbers")?;
        smoke_require(
            this,
            "command palette command executes and closes",
            |window| {
                !window.show_command_palette
                    && window.show_line_numbers != palette_line_numbers_before
            },
        )?;

        let initial_line_numbers = smoke_read(this, |window| window.show_line_numbers)?;
        smoke_activate_action(this, "line-numbers", &mut steps)?;
        smoke_require(this, "line numbers toggle changes state", |window| {
            window.show_line_numbers != initial_line_numbers
        })?;
        smoke_activate_action(this, "line-numbers", &mut steps)?;
        smoke_require(this, "line numbers toggle restores state", |window| {
            window.show_line_numbers == initial_line_numbers
        })?;

        let initial_whitespace = smoke_read(this, |window| window.app.settings().show_whitespace)?;
        smoke_activate_action(this, "visible-whitespace", &mut steps)?;
        smoke_require(this, "marks toggle changes state", |window| {
            window.app.settings().show_whitespace != initial_whitespace
        })?;
        smoke_activate_action(this, "visible-whitespace", &mut steps)?;
        smoke_require(this, "marks toggle restores state", |window| {
            window.app.settings().show_whitespace == initial_whitespace
        })?;

        let initial_word_wrap = smoke_read(this, |window| window.app.settings().word_wrap)?;
        smoke_activate_action(this, "word-wrap", &mut steps)?;
        smoke_require(this, "word wrap toggle changes state", |window| {
            window.app.settings().word_wrap != initial_word_wrap
        })?;
        smoke_activate_action(this, "word-wrap", &mut steps)?;
        smoke_require(this, "word wrap toggle restores state", |window| {
            window.app.settings().word_wrap == initial_word_wrap
        })?;

        smoke_activate_action(this, "tab-size-2", &mut steps)?;
        smoke_require(this, "tab size 2 applies", |window| {
            window.app.settings().tab_size == 2
        })?;
        smoke_activate_action(this, "tab-size-4", &mut steps)?;
        smoke_require(this, "tab size 4 applies", |window| {
            window.app.settings().tab_size == 4
        })?;
        smoke_activate_action(this, "tab-size-8", &mut steps)?;
        smoke_require(this, "tab size 8 applies", |window| {
            window.app.settings().tab_size == 8
        })?;
        smoke_activate_action(this, "tab-size-4", &mut steps)?;
        smoke_require(this, "tab size restores to 4", |window| {
            window.app.settings().tab_size == 4
        })?;

        for theme in ThemeMode::options() {
            let action_name = theme_action_name(*theme);
            smoke_activate_action(this, action_name, &mut steps)?;
            smoke_require(
                this,
                &format!("{} theme applies", theme.display_name()),
                |window| window.app.settings().theme == *theme,
            )?;
        }
        smoke_activate_action(this, "theme-system", &mut steps)?;
        smoke_require(this, "system theme restored", |window| {
            window.app.settings().theme == ThemeMode::System
        })?;

        for (index, command) in EditorCommandId::SHORTCUT_COMMANDS
            .iter()
            .copied()
            .enumerate()
        {
            smoke_activate_action(this, &format!("shortcut-{index}-disable"), &mut steps)?;
            smoke_require(
                this,
                &format!("{command:?} shortcut disable applies"),
                |window| {
                    window
                        .app
                        .settings()
                        .shortcuts
                        .shortcut_for(command)
                        .is_none()
                },
            )?;
            smoke_activate_action(this, &format!("shortcut-{index}-default"), &mut steps)?;
            smoke_require(
                this,
                &format!("{command:?} shortcut default restores"),
                |window| {
                    window.app.settings().shortcuts.shortcut_for(command)
                        == command.default_shortcut()
                },
            )?;
        }
        let close_tab_index = EditorCommandId::SHORTCUT_COMMANDS
            .iter()
            .position(|command| *command == EditorCommandId::CloseTab)
            .ok_or_else(|| "Close shortcut command was not found".to_string())?;
        let new_file_index = EditorCommandId::SHORTCUT_COMMANDS
            .iter()
            .position(|command| *command == EditorCommandId::NewFile)
            .ok_or_else(|| "New shortcut command was not found".to_string())?;
        smoke_activate_action(
            this,
            &format!("shortcut-{close_tab_index}-capture"),
            &mut steps,
        )?;
        smoke_require(this, "shortcut capture applies Ctrl+F4", |window| {
            window
                .app
                .settings()
                .shortcuts
                .shortcut_for(EditorCommandId::CloseTab)
                == Some(KeyboardShortcut::CTRL_F4)
        })?;
        smoke_require_action_accels(this, "win.close-tab", &["<Control>F4"], &mut steps)?;

        smoke_set_yes_no_prompt_decision(this, DialogChoice::No)?;
        smoke_activate_action(
            this,
            &format!("shortcut-{new_file_index}-capture"),
            &mut steps,
        )?;
        smoke_require(
            this,
            "shortcut duplicate prompt can keep existing owner",
            |window| {
                window
                    .app
                    .settings()
                    .shortcuts
                    .shortcut_for(EditorCommandId::CloseTab)
                    == Some(KeyboardShortcut::CTRL_F4)
                    && window
                        .app
                        .settings()
                        .shortcuts
                        .shortcut_for(EditorCommandId::NewFile)
                        == EditorCommandId::NewFile.default_shortcut()
            },
        )?;
        smoke_set_yes_no_prompt_decision(this, DialogChoice::Yes)?;
        smoke_activate_action(
            this,
            &format!("shortcut-{new_file_index}-capture"),
            &mut steps,
        )?;
        smoke_require(this, "shortcut duplicate prompt moves shortcut", |window| {
            window
                .app
                .settings()
                .shortcuts
                .shortcut_for(EditorCommandId::NewFile)
                == Some(KeyboardShortcut::CTRL_F4)
                && window
                    .app
                    .settings()
                    .shortcuts
                    .shortcut_for(EditorCommandId::CloseTab)
                    != Some(KeyboardShortcut::CTRL_F4)
        })?;
        smoke_require_action_accels(this, "win.new", &["<Control>F4"], &mut steps)?;
        smoke_require_action_accels(this, "win.close-tab", &[], &mut steps)?;

        {
            let mut window = this.try_borrow_mut().map_err(|_| {
                "window state was busy while preparing shortcut default duplicate smoke".to_string()
            })?;
            let mut settings = window.app.settings().clone();
            settings.shortcuts.set_shortcut(
                EditorCommandId::NewFile,
                EditorCommandId::CloseTab.default_shortcut(),
            );
            settings
                .shortcuts
                .set_shortcut(EditorCommandId::CloseTab, None);
            window.apply_settings(settings);
        }
        smoke_require(
            this,
            "shortcut default duplicate smoke owns close default elsewhere",
            |window| {
                window
                    .app
                    .settings()
                    .shortcuts
                    .shortcut_for(EditorCommandId::NewFile)
                    == EditorCommandId::CloseTab.default_shortcut()
                    && window
                        .app
                        .settings()
                        .shortcuts
                        .shortcut_for(EditorCommandId::CloseTab)
                        .is_none()
            },
        )?;

        smoke_set_yes_no_prompt_decision(this, DialogChoice::No)?;
        smoke_activate_action(
            this,
            &format!("shortcut-{close_tab_index}-default"),
            &mut steps,
        )?;
        smoke_require(
            this,
            "shortcut default duplicate prompt can keep existing owner",
            |window| {
                window
                    .app
                    .settings()
                    .shortcuts
                    .shortcut_for(EditorCommandId::NewFile)
                    == EditorCommandId::CloseTab.default_shortcut()
                    && window
                        .app
                        .settings()
                        .shortcuts
                        .shortcut_for(EditorCommandId::CloseTab)
                        .is_none()
            },
        )?;
        smoke_set_yes_no_prompt_decision(this, DialogChoice::Yes)?;
        smoke_activate_action(
            this,
            &format!("shortcut-{close_tab_index}-default"),
            &mut steps,
        )?;
        smoke_require(
            this,
            "shortcut default duplicate prompt moves shortcut",
            |window| {
                window
                    .app
                    .settings()
                    .shortcuts
                    .shortcut_for(EditorCommandId::CloseTab)
                    == EditorCommandId::CloseTab.default_shortcut()
                    && window
                        .app
                        .settings()
                        .shortcuts
                        .shortcut_for(EditorCommandId::NewFile)
                        != EditorCommandId::CloseTab.default_shortcut()
            },
        )?;

        smoke_activate_action(
            this,
            &format!("shortcut-{new_file_index}-default"),
            &mut steps,
        )?;
        smoke_require(
            this,
            "shortcut duplicate smoke restores new default",
            |window| {
                window
                    .app
                    .settings()
                    .shortcuts
                    .shortcut_for(EditorCommandId::NewFile)
                    == EditorCommandId::NewFile.default_shortcut()
            },
        )?;
        smoke_require_action_accels(this, "win.new", &["<Control>n"], &mut steps)?;

        smoke_activate_action(
            this,
            &format!("shortcut-{close_tab_index}-default"),
            &mut steps,
        )?;
        smoke_require(this, "shortcut capture can restore default", |window| {
            window
                .app
                .settings()
                .shortcuts
                .shortcut_for(EditorCommandId::CloseTab)
                == EditorCommandId::CloseTab.default_shortcut()
        })?;
        smoke_require_action_accels(this, "win.close-tab", &["<Control>w"], &mut steps)?;

        smoke_activate_action(this, "find", &mut steps)?;
        smoke_require(this, "find action opens find bar", |window| {
            window.show_find_bar
        })?;
        smoke_focus_find_entry_with_existing_query(this)?;
        smoke_run_find_entry_search(this, &mut steps)?;
        smoke_activate_action(this, "find-next", &mut steps)?;
        smoke_activate_action(this, "find-previous", &mut steps)?;
        smoke_activate_action(this, "find-all", &mut steps)?;
        smoke_activate_action(this, "replace", &mut steps)?;
        smoke_require(this, "replace action keeps find bar open", |window| {
            window.show_find_bar
        })?;
        smoke_activate_action(this, "close-find", &mut steps)?;
        smoke_require(this, "close find action hides find bar", |window| {
            !window.show_find_bar
        })?;

        let initial_count = smoke_read(this, |window| window.app.document_count())?;
        smoke_activate_action(this, "new", &mut steps)?;
        smoke_activate_action(this, "new", &mut steps)?;
        smoke_require(this, "new action creates tabs", |window| {
            window.app.document_count() == initial_count + 2
        })?;
        smoke_activate_action(this, "tab-left", &mut steps)?;
        smoke_activate_action(this, "tab-right", &mut steps)?;
        smoke_activate_action(this, "close-other-tabs", &mut steps)?;
        smoke_require(this, "close others leaves one tab", |window| {
            window.app.document_count() == 1
        })?;
        smoke_activate_action(this, "close-all-tabs", &mut steps)?;
        smoke_require(this, "close all recreates one clean tab", |window| {
            window.app.document_count() == 1
                && window
                    .app
                    .current_document()
                    .is_some_and(|document| !document.is_dirty())
        })?;
        smoke_activate_action(this, "close-tab", &mut steps)?;
        smoke_require(this, "close tab recreates one clean tab", |window| {
            window.app.document_count() == 1
                && window
                    .app
                    .current_document()
                    .is_some_and(|document| !document.is_dirty())
        })?;
        smoke_require_action_enabled(this, "save", true)?;
        smoke_require_action_enabled(this, "save-as", true)?;

        smoke_insert_text(this, "replace-source replace-source replace-source")?;
        smoke_set_find_replace(this, "replace-source", "replace-one")?;
        smoke_activate_action(this, "find-next", &mut steps)?;
        smoke_activate_action(this, "replace-current", &mut steps)?;
        smoke_require(
            this,
            "replace current changes one selected match",
            |window| {
                let text = window.buffer_text();
                text.contains("replace-one") && text.matches("replace-source").count() == 2
            },
        )?;
        smoke_set_find_replace(this, "replace-source", "replace-all")?;
        smoke_activate_action(this, "replace-all", &mut steps)?;
        smoke_require(this, "replace all changes remaining matches", |window| {
            let text = window.buffer_text();
            text.contains("replace-one")
                && text.matches("replace-all").count() == 2
                && !text.contains("replace-source")
        })?;

        smoke_activate_action(this, "line-ending-lf", &mut steps)?;
        smoke_require(this, "LF line ending action applies", |window| {
            window
                .app
                .current_document()
                .is_some_and(|document| document.line_ending() == LineEnding::Lf)
        })?;
        smoke_activate_action(this, "line-ending-cr", &mut steps)?;
        smoke_require(this, "CR line ending action applies", |window| {
            window
                .app
                .current_document()
                .is_some_and(|document| document.line_ending() == LineEnding::Cr)
        })?;
        smoke_activate_action(this, "line-ending-crlf", &mut steps)?;
        smoke_require(this, "CRLF line ending action applies", |window| {
            window
                .app
                .current_document()
                .is_some_and(|document| document.line_ending() == LineEnding::Crlf)
        })?;

        smoke_insert_text(this, "\nlinux action smoke edit")?;
        smoke_require(this, "undo action becomes enabled after edit", |window| {
            window
                .actions
                .get("undo")
                .is_some_and(|action| action.is_enabled())
        })?;
        smoke_activate_action(this, "undo", &mut steps)?;
        smoke_require(this, "undo action removes smoke edit", |window| {
            !window.buffer_text().contains("linux action smoke edit")
        })?;
        smoke_require(this, "redo action becomes enabled after undo", |window| {
            window
                .actions
                .get("redo")
                .is_some_and(|action| action.is_enabled())
        })?;
        smoke_activate_action(this, "redo", &mut steps)?;
        smoke_require(this, "redo action restores smoke edit", |window| {
            window.buffer_text().contains("linux action smoke edit")
        })?;
        smoke_activate_action(this, "select-all", &mut steps)?;
        smoke_activate_action(this, "copy", &mut steps)?;
        smoke_activate_action(this, "cut", &mut steps)?;
        smoke_require(this, "cut action clears selected text", |window| {
            window.buffer_text().is_empty()
        })?;
        smoke_activate_action(this, "paste", &mut steps)?;
        smoke_require(this, "paste action restores cut plain text", |window| {
            window.buffer_text().contains("linux action smoke edit")
        })?;
        smoke_set_mixed_clipboard(this, "plain clipboard text", "<b>html clipboard text</b>")?;
        smoke_activate_action(this, "select-all", &mut steps)?;
        smoke_activate_action(this, "paste", &mut steps)?;
        smoke_require(this, "rich clipboard paste uses plain text", |window| {
            window.buffer_text() == "plain clipboard text"
        })?;

        smoke_activate_action(this, "about", &mut steps)?;

        smoke_activate_action(this, "tab-size-8", &mut steps)?;
        smoke_require(this, "persisted tab size applies", |window| {
            window.app.settings().tab_size == 8
        })?;
        smoke_activate_action(this, "theme-light", &mut steps)?;
        smoke_require(this, "persisted theme applies", |window| {
            window.app.settings().theme == ThemeMode::Light
        })?;
        smoke_activate_action(
            this,
            &format!("shortcut-{close_tab_index}-capture"),
            &mut steps,
        )?;
        smoke_require(this, "persisted shortcut capture applies", |window| {
            window
                .app
                .settings()
                .shortcuts
                .shortcut_for(EditorCommandId::CloseTab)
                == Some(KeyboardShortcut::CTRL_F4)
        })?;
        smoke_flush_persistence(this)?;

        Ok(steps)
    }

    fn restore_startup_state(&mut self) -> Result<(), AppError> {
        let restored_documents = if self.startup_paths.is_empty() {
            false
        } else {
            self.restore_startup_paths()
        };
        if !restored_documents && self.app.document_count() == 0 {
            self.app.new_document();
        }
        self.refresh_tabs();
        self.load_current_document_into_edit()?;
        self.update_status();
        self.update_line_numbers();
        self.show_startup_warnings();
        Ok(())
    }

    fn restore_startup_paths(&mut self) -> bool {
        let startup_paths = std::mem::take(&mut self.startup_paths);
        let mut restored = false;
        for path in startup_paths {
            match self.open_startup_path(path.clone()) {
                Ok(opened) => restored |= opened,
                Err(error) => self.startup_warnings.push(format!(
                    "Could not open {}: {}",
                    path.display(),
                    error.user_message()
                )),
            }
        }
        restored
    }

    fn rebuild_menu(&self) {
        self.menu_bar
            .set_menu_model(Some(&build_menu_model(&self.app)));
    }

    fn rebuild_menu_later(&self) {
        let menu_bar = self.menu_bar.clone();
        let model = build_menu_model(&self.app);
        glib::idle_add_local_once(move || {
            menu_bar.set_menu_model(Some(&model));
        });
    }

    fn show_tab_context_menu(&mut self, x: f64, y: f64) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        let Some(tab_index) = self.tab_index_at_notebook_point(x, y) else {
            return Ok(());
        };

        self.select_tab(tab_index)?;

        let model = build_tab_context_menu_model(&self.app);
        let popover = gtk::PopoverMenu::from_model(Some(&model));
        popover.set_parent(&self.notebook);
        popover.set_has_arrow(false);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(
            x.round() as i32,
            y.round() as i32,
            1,
            1,
        )));
        popover.popup();
        Ok(())
    }

    fn tab_index_at_notebook_point(&self, x: f64, y: f64) -> Option<usize> {
        for index in 0..self.app.document_count() {
            let Some(page) = self.notebook.nth_page(Some(index as u32)) else {
                continue;
            };
            let Some(tab) = self.notebook.tab_label(&page) else {
                continue;
            };
            let Some(bounds) = tab.compute_bounds(&self.notebook) else {
                continue;
            };
            if point_in_rect(
                x,
                y,
                bounds.x() as f64,
                bounds.y() as f64,
                bounds.width() as f64,
                bounds.height() as f64,
            ) {
                return Some(index);
            }
        }
        None
    }

    fn show_editor_context_menu(&mut self, x: f64, y: f64) {
        self.update_status();
        self.update_menu_checks();
        focus_text_view(&self.window, &self.text_view);
        let model = build_editor_context_menu_model(&self.app);
        let popover = gtk::PopoverMenu::from_model(Some(&model));
        {
            let window = self.window.clone();
            let text_view = self.text_view.clone();
            popover.connect_closed(move |_| {
                let idle_window = window.clone();
                let idle_text_view = text_view.clone();
                glib::idle_add_local_once(move || {
                    focus_text_view(&idle_window, &idle_text_view);
                });
            });
        }
        popover.set_parent(&self.text_view);
        popover.set_has_arrow(false);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(
            x.round() as i32,
            y.round() as i32,
            1,
            1,
        )));
        popover.popup();
    }

    fn open_startup_path(&mut self, path: PathBuf) -> Result<bool, AppError> {
        let document_count = self.app.document_count();
        match self.open_path_as_new_tab(path.clone(), None) {
            Ok(()) => Ok(self.app.document_count() > document_count),
            Err(error) if error.file_access_kind() == Some(FileAccessKind::NotFound) => {
                self.open_missing_startup_path(path)
            }
            Err(error) => Err(error),
        }
    }

    fn open_missing_startup_path(&mut self, path: PathBuf) -> Result<bool, AppError> {
        let message = format!("{} not found.\n\nCreate it?", path.display());
        if !matches!(self.ask_yes_no("File Missing", &message), DialogChoice::Yes) {
            return Ok(false);
        }
        self.sync_current_text()?;
        self.app.new_document_for_path(path);
        self.refresh_tabs();
        self.load_current_document_into_edit()?;
        self.persist_recent_files_report_errors();
        Ok(true)
    }

    fn handle_status_timer(&mut self) {
        if modal_dialog_active() {
            return;
        }
        self.poll_pending_save_report_errors();
        self.flush_ready_persistence_report_errors();
    }

    fn new_tab(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        self.app.new_document();
        self.refresh_tabs();
        self.load_current_document_into_edit()
    }

    fn open_file(&mut self, requested_encoding: Option<TextEncoding>) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        if !self.confirm_current_dirty("open")? {
            return Ok(());
        }
        if let Some(path) = self.open_file_dialog()? {
            self.open_path_as_new_tab(path, requested_encoding)?;
        }
        Ok(())
    }

    fn open_recent_file(&mut self, index: usize) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        let Some(path) = self.app.recent_files().get(index).cloned() else {
            return Ok(());
        };
        if !path.exists() {
            self.app.remove_recent_file(&path);
            self.rebuild_menu_later();
            self.show_warning("Recent", "Recent file is gone. It was removed.");
            self.persist_recent_files_report_errors();
            return Ok(());
        }
        if !self.confirm_current_dirty("open")? {
            return Ok(());
        }
        self.open_path_as_new_tab(path, None)
    }

    fn select_existing_document_for_path(&mut self, path: &Path) -> Result<bool, AppError> {
        let Some(index) = self.document_index_for_path(path) else {
            return Ok(false);
        };
        let recent_path = self
            .app
            .documents()
            .get(index)
            .filter(|document| !document.backing_file_missing())
            .and_then(|document| document.path().cloned());

        if self.app.current_index() != Some(index) {
            self.select_tab(index)?;
        }
        if let Some(recent_path) = recent_path {
            self.app.record_recent_file(recent_path);
            self.rebuild_menu();
            self.persist_recent_files_report_errors();
        }
        Ok(true)
    }

    fn open_path_as_new_tab(
        &mut self,
        path: PathBuf,
        requested_encoding: Option<TextEncoding>,
    ) -> Result<(), AppError> {
        if self.select_existing_document_for_path(&path)? {
            return Ok(());
        }
        let Some(confirmed_policy) = self.confirm_large_file_policy(&path)? else {
            return Ok(());
        };
        let loaded = self.io.load_with_metadata_and_prechecked_len(
            &path,
            requested_encoding,
            confirmed_policy.read_only_reason,
            confirmed_policy.byte_len,
        )?;
        self.sync_current_text()?;
        self.app
            .open_document_with_metrics(loaded.document, loaded.metrics);
        self.rebuild_menu();
        self.refresh_tabs();
        self.load_current_document_into_edit()?;
        self.persist_recent_files_report_errors();
        Ok(())
    }

    fn open_current_tab_in_new_window(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        if self.app.document_count() <= 1 {
            return Err(AppError::InvalidState(
                "Open another tab before opening this one in a new window.",
            ));
        }
        self.sync_current_text()?;
        let is_dirty = self
            .app
            .current_document()
            .is_some_and(|document| document.is_dirty());
        if is_dirty
            && !self.confirm_current_dirty("opening in a new window and closing this tab")?
        {
            return Ok(());
        }
        let path = self
            .app
            .current_document()
            .ok_or(AppError::InvalidState("No file open."))?
            .path()
            .cloned()
            .ok_or(AppError::InvalidState(
                "Save this tab before opening it in a new window.",
            ))?;
        launch_document_in_new_window(&path)?;
        self.close_current_tab_after_confirmed()
    }

    fn reopen_current_file_with_encoding_dialog(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        let current = {
            let document = self
                .app
                .current_document()
                .ok_or(AppError::InvalidState("No file open."))?;
            if document.path().is_none() {
                return Err(AppError::InvalidState("No file path."));
            }
            document.encoding()
        };
        let Some(encoding) = self.choose_encoding_dialog(
            "Reopen",
            "Pick encoding",
            "File will reopen with it.",
            current,
        )?
        else {
            return Ok(());
        };
        self.reopen_current_file_as(encoding)
    }

    fn reopen_current_file_as(&mut self, encoding: TextEncoding) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        let path = self
            .app
            .current_document()
            .ok_or(AppError::InvalidState("No file open."))?
            .path()
            .cloned()
            .ok_or(AppError::InvalidState("No file path."))?;
        if self
            .app
            .current_document()
            .is_some_and(|document| document.is_dirty())
            && !self.confirm_current_dirty("reload")?
        {
            return Ok(());
        }
        let Some(confirmed_policy) = self.confirm_large_file_policy(&path)? else {
            return Ok(());
        };
        let loaded = self.io.load_with_metadata_and_prechecked_len(
            &path,
            Some(encoding),
            confirmed_policy.read_only_reason,
            confirmed_policy.byte_len,
        )?;
        self.app
            .replace_current_document_with_metrics(loaded.document, loaded.metrics)?;
        self.rebuild_menu();
        self.refresh_current_tab_display();
        self.load_current_document_into_edit()?;
        self.persist_recent_files_report_errors();
        Ok(())
    }

    fn reload_current_file_after_external_save_conflict(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        let path = self
            .app
            .current_document()
            .ok_or(AppError::InvalidState("No file open."))?
            .path()
            .cloned()
            .ok_or(AppError::InvalidState("No file path."))?;
        let Some(confirmed_policy) = self.confirm_large_file_policy(&path)? else {
            return Ok(());
        };
        let loaded = self.io.load_with_metadata_and_prechecked_len(
            &path,
            None,
            confirmed_policy.read_only_reason,
            confirmed_policy.byte_len,
        )?;
        self.app
            .replace_current_document_with_metrics(loaded.document, loaded.metrics)?;
        self.rebuild_menu();
        self.refresh_current_tab_display();
        self.load_current_document_into_edit()?;
        self.persist_recent_files_report_errors();
        Ok(())
    }

    fn report_error(&mut self, error: AppError) {
        match error {
            AppError::ExternalFileChanged { path } => {
                if let Err(error) = self.resolve_external_file_changed(path) {
                    self.report_error(error);
                }
            }
            error => self.show_error(&error),
        }
    }

    fn resolve_external_file_changed(&mut self, path: PathBuf) -> Result<(), AppError> {
        let Some(index) = self.document_index_for_path(&path) else {
            self.show_error(&AppError::external_file_changed(path));
            return Ok(());
        };
        if self.app.current_index() != Some(index) {
            self.select_tab(index)?;
        }

        match self.ask_external_file_changed_action(&path) {
            ExternalFileChangedAction::Reload => {
                self.reload_current_file_after_external_save_conflict()
            }
            ExternalFileChangedAction::SaveAs => self
                .save_current_with_mode(true, SaveMode::Background)
                .map(|_| ()),
            ExternalFileChangedAction::Cancel => Ok(()),
        }
    }

    fn document_index_for_path(&self, path: &Path) -> Option<usize> {
        self.app.document_index_for_path(path)
    }

    fn convert_current_encoding_dialog(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        self.ensure_current_writable()?;
        let current = self
            .app
            .current_document()
            .ok_or(AppError::InvalidState("No file open."))?
            .encoding();
        let Some(encoding) = self.choose_encoding_dialog(
            "Change Encoding",
            "Pick encoding",
            "Next save will use it.",
            current,
        )?
        else {
            return Ok(());
        };
        self.convert_current_encoding(encoding)
    }

    fn convert_current_encoding(&mut self, encoding: TextEncoding) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        self.ensure_current_writable()?;
        let content = self
            .app
            .current_document()
            .ok_or(AppError::InvalidState("No file open."))?
            .content_snapshot();
        if !encoding.can_encode_all_unicode()
            && let Err(error) = self.io.ensure_encodable(content.as_ref(), encoding)
        {
            self.show_warning(
                "Encoding",
                &format!("{}\n\nPick another encoding.", error.user_message()),
            );
            return Ok(());
        }
        self.set_encoding(encoding)
    }

    fn save_current_command(&mut self, force_save_as: bool) -> Result<(), AppError> {
        self.save_current_with_mode(force_save_as, SaveMode::Background)
            .map(|_| ())
    }

    fn save_current_for_dirty_prompt(&mut self, force_save_as: bool) -> Result<bool, AppError> {
        self.save_current_with_mode(
            force_save_as,
            SaveMode::Blocking {
                reload_current_document_into_edit: false,
            },
        )
    }

    fn save_current_with_mode(
        &mut self,
        force_save_as: bool,
        mode: SaveMode,
    ) -> Result<bool, AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        if self.app.settings().show_whitespace && !force_save_as {
            return Err(AppError::InvalidState("Turn off marks, or use Save As."));
        }
        let (
            document_id,
            document_path,
            document_snapshot,
            backing_file_missing,
            mut encoding,
            line_ending,
        ) = {
            let document = self
                .app
                .current_document()
                .ok_or(AppError::InvalidState("No file open."))?;
            if document.is_read_only() && !force_save_as {
                return Err(AppError::InvalidState("This is read-only. Use Save As."));
            }
            (
                document.id(),
                document.path().cloned(),
                document.snapshot(),
                document.backing_file_missing(),
                document.encoding(),
                document.line_ending(),
            )
        };
        let needs_save_as = force_save_as || document_path.is_none();
        let save_path = match (!force_save_as).then(|| document_path.clone()).flatten() {
            Some(path) => path,
            None => match self.save_file_dialog()? {
                Some(path) => path,
                None => return Ok(false),
            },
        };
        let selected_target_expectation = if needs_save_as {
            let expectation = selected_save_target_expectation(&self.io, save_path.as_path())?;
            if !self.confirm_save_as_overwrite(save_path.as_path(), expectation) {
                return Ok(false);
            }
            Some(expectation)
        } else {
            None
        };
        if needs_save_as {
            let Some(selected_encoding) = self.choose_encoding_dialog(
                "Save",
                "Pick encoding",
                "Saved file will use it.",
                encoding,
            )?
            else {
                return Ok(false);
            };
            encoding = selected_encoding;
        }
        let content = self
            .app
            .current_document()
            .ok_or(AppError::InvalidState("No file open."))?
            .content_snapshot();
        let target_expectation = save_target_expectation(
            document_path.as_deref(),
            save_path.as_path(),
            document_snapshot,
            backing_file_missing,
            force_save_as,
            selected_target_expectation,
        )?;

        match mode {
            SaveMode::Blocking {
                reload_current_document_into_edit,
            } => {
                let saved = self.io.save_with_target_expectation_and_metadata(
                    &save_path,
                    content.as_ref(),
                    encoding,
                    line_ending,
                    target_expectation,
                )?;
                self.app.mark_document_saved(
                    document_id,
                    save_path.clone(),
                    encoding,
                    line_ending,
                    Some(saved.snapshot),
                )?;
                self.rebuild_menu();
                self.refresh_current_tab_display();
                if reload_current_document_into_edit {
                    self.load_current_document_into_edit()?;
                } else {
                    self.buffer.set_modified(false);
                    self.edit_content_pending_sync = false;
                    self.apply_current_read_only();
                    self.update_line_numbers();
                    self.update_menu_checks();
                }
                self.update_status();
                self.persist_recent_files_report_errors();
                self.flush_pending_persistence_report_errors();
            }
            SaveMode::Background => {
                self.start_background_save(
                    document_id,
                    save_path,
                    encoding,
                    line_ending,
                    target_expectation,
                    content,
                )?;
            }
        }
        Ok(true)
    }

    fn start_background_save(
        &mut self,
        document_id: crate::domain::DocumentId,
        path: PathBuf,
        encoding: TextEncoding,
        line_ending: LineEnding,
        target_expectation: SaveTargetExpectation,
        content: Arc<str>,
    ) -> Result<(), AppError> {
        let (sender, receiver) = mpsc::channel();
        let worker_path = path.clone();
        thread::Builder::new()
            .spawn(move || {
                let io = FileDocumentIo::new();
                let result = io.save_with_target_expectation_and_metadata(
                    &worker_path,
                    content.as_ref(),
                    encoding,
                    line_ending,
                    target_expectation,
                );
                let _ = sender.send(result);
            })
            .map_err(|source| AppError::io(source, "start background save worker"))?;
        self.pending_save = Some(PendingSave {
            document_id,
            path,
            encoding,
            line_ending,
            receiver,
        });
        self.app.set_saving_document(Some(document_id));
        self.apply_current_read_only();
        self.update_status();
        self.update_menu_checks();
        Ok(())
    }

    fn ensure_no_pending_save(&mut self) -> Result<(), AppError> {
        self.poll_pending_save()?;
        if self.pending_save.is_some() {
            return Err(AppError::InvalidState(
                "Save is still running. Please wait.",
            ));
        }
        Ok(())
    }

    fn poll_pending_save_report_errors(&mut self) {
        if let Err(error) = self.poll_pending_save() {
            self.report_error(error);
        }
    }

    fn poll_pending_save(&mut self) -> Result<(), AppError> {
        let Some(result) = self.pending_save_result() else {
            return Ok(());
        };
        let pending = self
            .pending_save
            .take()
            .ok_or(AppError::InvalidState("No save to finish."))?;
        self.complete_pending_save(pending, result)
    }

    fn pending_save_result(&self) -> Option<Result<SavedFileSnapshot, AppError>> {
        match self
            .pending_save
            .as_ref()
            .map(|pending| pending.receiver.try_recv())
        {
            Some(Ok(result)) => Some(result),
            Some(Err(TryRecvError::Empty)) | None => None,
            Some(Err(TryRecvError::Disconnected)) => Some(Err(AppError::InvalidState(
                "Save stopped before it finished.",
            ))),
        }
    }

    fn complete_pending_save(
        &mut self,
        pending: PendingSave,
        result: Result<SavedFileSnapshot, AppError>,
    ) -> Result<(), AppError> {
        self.app.set_saving_document(None);
        match result {
            Ok(saved) => {
                self.app.mark_document_saved(
                    pending.document_id,
                    pending.path,
                    pending.encoding,
                    pending.line_ending,
                    Some(saved.snapshot),
                )?;
                self.rebuild_menu();
                self.refresh_tab_display_for_document(pending.document_id);
                self.apply_current_read_only();
                self.update_status();
                self.update_menu_checks();
                self.persist_recent_files_report_errors();
                self.flush_pending_persistence_report_errors();
                Ok(())
            }
            Err(error) => {
                self.apply_current_read_only();
                self.update_status();
                self.update_menu_checks();
                Err(error)
            }
        }
    }

    fn close_current_tab(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        if !self.confirm_current_dirty("close")? {
            return Ok(());
        }
        self.close_current_tab_after_confirmed()
    }

    fn close_current_tab_after_confirmed(&mut self) -> Result<(), AppError> {
        if let Some(document) = self.app.remove_current_document() {
            self.editor_view_states.remove(&document.id());
            self.visible_whitespace_display_cache.remove(&document.id());
        }
        if self.app.document_count() == 0 {
            self.app.new_document();
        }
        self.refresh_tabs();
        self.load_current_document_into_edit()?;
        self.persist_recent_files_report_errors();
        Ok(())
    }

    fn close_other_tabs(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        let Some(current_index) = self.app.current_index() else {
            return Ok(());
        };
        let dirty_indices = self
            .app
            .dirty_indices()
            .into_iter()
            .filter(|index| *index != current_index)
            .collect::<Vec<_>>();
        if !self.confirm_dirty_indices(&dirty_indices, "close")? {
            self.select_tab(current_index)?;
            return Ok(());
        }
        self.select_tab(current_index)?;
        if self.app.remove_other_documents() {
            self.retain_open_editor_view_states();
            self.retain_open_visible_whitespace_display_cache();
            self.refresh_tabs();
            self.load_current_document_into_edit()?;
            self.persist_recent_files_report_errors();
        }
        Ok(())
    }

    fn close_all_tabs(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        let dirty_indices = self.app.dirty_indices();
        if !self.confirm_dirty_indices(&dirty_indices, "close")? {
            return Ok(());
        }
        self.app.remove_all_documents();
        self.editor_view_states.clear();
        self.visible_whitespace_display_cache.clear();
        self.app.new_document();
        self.refresh_tabs();
        self.load_current_document_into_edit()?;
        self.persist_recent_files_report_errors();
        Ok(())
    }

    fn set_encoding(&mut self, encoding: TextEncoding) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.ensure_current_writable()?;
        self.app.set_current_encoding(encoding)?;
        self.refresh_current_tab_display();
        self.update_status();
        self.update_menu_checks();
        Ok(())
    }

    fn set_line_ending(&mut self, line_ending: LineEnding) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.ensure_current_writable()?;
        self.app.set_current_line_ending(line_ending)?;
        self.refresh_current_tab_display();
        self.update_status();
        self.update_menu_checks();
        Ok(())
    }

    fn sync_current_text(&mut self) -> Result<(), AppError> {
        if self.programmatic_update {
            return Ok(());
        }
        self.remember_current_view_state();
        if self.app.settings().show_whitespace || !self.edit_content_pending_sync {
            return Ok(());
        }
        if self.current_document_is_saving() {
            self.edit_content_pending_sync = false;
            return Ok(());
        }
        if self
            .app
            .current_document()
            .is_some_and(|document| document.is_read_only())
        {
            return Ok(());
        }
        let text = self.buffer_text();
        let scan = LoadedTextAnalysis::scan_text(&text);
        if scan.contains_nul {
            return Err(AppError::encoding_unsafe_text(
                "GTK text buffer contains NUL text",
            ));
        }
        self.app
            .update_current_changed_view_content_with_metrics(text, scan.analysis.metrics)?;
        self.edit_content_pending_sync = false;
        Ok(())
    }

    fn on_text_changed(&mut self) -> Result<(), AppError> {
        if self.programmatic_update || self.current_document_is_saving() {
            return Ok(());
        }
        self.edit_content_pending_sync = true;
        let dirty_changed = self.app.mark_current_dirty_from_view()?;
        self.app.clear_search_results();
        self.show_search_results = false;
        self.show_or_hide_search_results();
        if dirty_changed {
            self.refresh_current_tab_display();
        }
        self.update_status();
        self.update_line_numbers();
        self.update_menu_checks();
        Ok(())
    }

    fn on_editor_surface_view_changed(&mut self) {
        if self.programmatic_update {
            return;
        }
        self.remember_current_view_state();
        self.refresh_current_editor_surface_state();
        self.update_status();
        self.update_line_numbers();
        self.update_menu_checks();
    }

    fn on_find_query_changed(&mut self) {
        if self.show_search_results || !self.app.search_results().is_empty() {
            self.app.clear_search_results();
            self.show_search_results = false;
            self.show_or_hide_search_results();
        }
    }

    fn format_tab_tooltip_path(path: &Path) -> String {
        Self::wrap_long_tab_tooltip_path(&path.display().to_string(), TAB_TOOLTIP_WRAP_COLUMN)
    }

    fn wrap_long_tab_tooltip_path(path: &str, max_line_chars: usize) -> String {
        if max_line_chars == 0 || path.chars().count() <= max_line_chars {
            return path.to_string();
        }

        let mut wrapped = String::with_capacity(path.len() + path.len() / max_line_chars * 2);
        let mut line_len = 0;
        let mut chunk = String::new();
        let mut chunk_len = 0;

        for ch in path.chars() {
            chunk.push(ch);
            chunk_len += 1;

            if Self::is_tab_tooltip_path_separator(ch) {
                Self::append_tab_tooltip_path_chunk(
                    &mut wrapped,
                    &mut line_len,
                    &chunk,
                    chunk_len,
                    max_line_chars,
                );
                chunk.clear();
                chunk_len = 0;
            }
        }

        if !chunk.is_empty() {
            Self::append_tab_tooltip_path_chunk(
                &mut wrapped,
                &mut line_len,
                &chunk,
                chunk_len,
                max_line_chars,
            );
        }

        wrapped
    }

    fn append_tab_tooltip_path_chunk(
        wrapped: &mut String,
        line_len: &mut usize,
        chunk: &str,
        chunk_len: usize,
        max_line_chars: usize,
    ) {
        if *line_len > 0 && *line_len + chunk_len > max_line_chars {
            wrapped.push_str("\r\n");
            *line_len = 0;
        }

        if chunk_len <= max_line_chars {
            wrapped.push_str(chunk);
            *line_len += chunk_len;
            return;
        }

        for ch in chunk.chars() {
            if *line_len >= max_line_chars {
                wrapped.push_str("\r\n");
                *line_len = 0;
            }
            wrapped.push(ch);
            *line_len += 1;
        }
    }

    fn is_tab_tooltip_path_separator(ch: char) -> bool {
        ch == '\\' || ch == '/'
    }

    fn refresh_tabs(&mut self) {
        self.programmatic_update = true;
        while self.notebook.n_pages() > 0 {
            self.notebook.remove_page(Some(0));
        }
        for document in self.app.documents() {
            let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let label = gtk::Label::new(Some(&document.tab_title()));
            let tooltip = document
                .path()
                .map(|path| Self::format_tab_tooltip_path(path.as_path()));
            label.set_tooltip_text(tooltip.as_deref());
            self.notebook.append_page(&page, Some(&label));
        }
        if let Some(index) = self.app.current_index() {
            self.notebook.set_current_page(Some(index as u32));
        }
        self.programmatic_update = false;
        self.update_menu_checks();
    }

    fn refresh_current_tab_display(&mut self) {
        if let Some(index) = self.app.current_index() {
            self.refresh_tab_display_at(index);
        }
        self.update_menu_checks();
    }

    fn refresh_tab_display_for_document(&mut self, document_id: crate::domain::DocumentId) {
        if let Some(index) = self
            .app
            .documents()
            .iter()
            .position(|document| document.id() == document_id)
        {
            self.refresh_tab_display_at(index);
        }
        self.update_menu_checks();
    }

    fn refresh_tab_display_at(&mut self, index: usize) {
        if let Some(page) = self.notebook.nth_page(Some(index as u32))
            && let Some(tab) = self.notebook.tab_label(&page)
            && let Ok(label) = tab.downcast::<gtk::Label>()
            && let Some(document) = self.app.documents().get(index)
        {
            label.set_text(&document.tab_title());
            let tooltip = document
                .path()
                .map(|path| Self::format_tab_tooltip_path(path.as_path()));
            label.set_tooltip_text(tooltip.as_deref());
        }
    }

    fn select_tab(&mut self, index: usize) -> Result<(), AppError> {
        if self.programmatic_update || self.app.current_index() == Some(index) {
            return Ok(());
        }
        let previous_index = self.app.current_index();
        let refresh_previous_tab = self.edit_content_pending_sync;
        self.sync_current_text()?;
        if refresh_previous_tab && let Some(previous_index) = previous_index {
            self.refresh_tab_display_at(previous_index);
        }
        self.app.set_current_index(index)?;
        self.programmatic_update = true;
        self.notebook.set_current_page(Some(index as u32));
        self.programmatic_update = false;
        self.refresh_current_tab_display();
        self.load_current_document_into_edit()
    }

    fn load_current_document_into_edit(&mut self) -> Result<(), AppError> {
        // Match Windows: this automatic fallback changes the current view only;
        // it does not persist the user's Marks setting.
        disable_visible_whitespace_for_oversized_current_document(&mut self.app);

        let show_whitespace = self.app.settings().show_whitespace;
        let text = match self.app.current_document() {
            Some(document) => {
                let source = document.content();
                let key = VisibleWhitespaceDisplayCacheKey::new(
                    document.id(),
                    document.content_generation(),
                    source.len(),
                    show_whitespace,
                );
                visible_whitespace_display_text(
                    &mut self.visible_whitespace_display_cache,
                    key,
                    source,
                )
            }
            None => {
                self.visible_whitespace_display_cache.clear();
                VisibleWhitespaceDisplayText::Source("")
            }
        };
        self.programmatic_update = true;
        set_buffer_text_without_undo(&self.buffer, text.as_str());
        self.programmatic_update = false;
        self.line_numbers_snapshot = None;
        self.edit_content_pending_sync = false;
        self.restore_current_view_state();
        self.apply_current_read_only();
        self.update_status();
        self.update_line_numbers();
        self.update_menu_checks();
        Ok(())
    }

    fn remember_current_view_state(&mut self) {
        let Some(document_id) = self.app.current_document().map(|document| document.id()) else {
            return;
        };
        let (start, end, had_selection) = self.selection_char_offsets();
        let vadjustment = self.editor_scrolled.vadjustment().value().round() as i32;
        self.editor_view_states.insert(
            document_id,
            GtkEditorViewState {
                selection_start: start,
                selection_end: end,
                vadjustment,
                had_selection,
            },
        );
    }

    fn restore_current_view_state(&mut self) {
        let Some(document_id) = self.app.current_document().map(|document| document.id()) else {
            return;
        };
        let Some(state) = self.editor_view_states.get(&document_id).copied() else {
            return;
        };
        let start = self.buffer.iter_at_offset(state.selection_start);
        let end = self.buffer.iter_at_offset(state.selection_end);
        if state.had_selection {
            self.buffer.select_range(&start, &end);
        } else {
            self.buffer.place_cursor(&start);
        }
        self.editor_scrolled
            .vadjustment()
            .set_value(f64::from(state.vadjustment.max(0)));
    }

    fn retain_open_editor_view_states(&mut self) {
        let open_document_ids = self
            .app
            .documents()
            .iter()
            .map(|document| document.id())
            .collect::<Vec<_>>();
        self.editor_view_states
            .retain(|document_id, _| open_document_ids.contains(document_id));
    }

    fn retain_open_visible_whitespace_display_cache(&mut self) {
        let open_document_ids = self
            .app
            .documents()
            .iter()
            .map(|document| document.id())
            .collect::<Vec<_>>();
        self.visible_whitespace_display_cache
            .retain(|document_id, _| open_document_ids.contains(document_id));
    }

    fn confirm_current_dirty(&mut self, action: &str) -> Result<bool, AppError> {
        self.sync_current_text()?;
        if !self
            .app
            .current_document()
            .is_some_and(|document| document.is_dirty())
        {
            return Ok(true);
        }
        let message = format!("Unsaved changes.\nSave before {action}?");
        match self.ask_save_discard_cancel("Unsaved", &message) {
            DialogChoice::Yes => self.save_current_for_dirty_prompt(false),
            DialogChoice::No => Ok(true),
            DialogChoice::Cancel => Ok(false),
        }
    }

    fn confirm_all_dirty_before_exit(&mut self) -> bool {
        if let Err(error) = self.poll_pending_save() {
            self.report_error(error);
            return false;
        }
        if self.pending_save.is_some() {
            self.show_info(SAVING_DIALOG_TITLE, SAVE_STILL_RUNNING_MESSAGE);
            return false;
        }
        if let Err(error) = self.sync_current_text() {
            self.report_error(error);
            return false;
        }
        let dirty_indices = self.app.dirty_indices();
        match self.confirm_dirty_indices(&dirty_indices, "exit") {
            Ok(confirmed) => confirmed,
            Err(error) => {
                self.report_error(error);
                false
            }
        }
    }

    fn confirm_dirty_indices(
        &mut self,
        dirty_indices: &[usize],
        action: &str,
    ) -> Result<bool, AppError> {
        for &index in dirty_indices {
            if index >= self.app.document_count() {
                continue;
            }
            self.select_tab(index)?;
            if !self.confirm_current_dirty(action)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn confirm_large_file_policy(
        &self,
        path: &Path,
    ) -> Result<Option<ConfirmedLargeFilePolicy>, AppError> {
        let byte_len = self.io.file_byte_len(path)?;
        if byte_len > MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES {
            return Err(AppError::file_too_large(
                path.to_path_buf(),
                byte_len,
                MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES,
            ));
        }
        if should_warn_large_file(byte_len) {
            let message = format!(
                "{} is {:.1} MB.\n\nLarge files open read-only. Continue?",
                path.display(),
                byte_len as f64 / (1024.0 * 1024.0)
            );
            if matches!(self.ask_yes_no("Large File", &message), DialogChoice::Yes) {
                return Ok(Some(ConfirmedLargeFilePolicy {
                    read_only_reason: Some(ReadOnlyReason::LargeFile),
                    byte_len,
                }));
            }
            return Ok(None);
        }
        if can_load_document_bytes(byte_len) {
            Ok(Some(ConfirmedLargeFilePolicy {
                read_only_reason: None,
                byte_len,
            }))
        } else {
            Err(AppError::file_too_large(
                path.to_path_buf(),
                byte_len,
                MAX_DOCUMENT_LOAD_BYTES,
            ))
        }
    }

    fn ensure_current_writable(&self) -> Result<(), AppError> {
        if self.app.settings().show_whitespace {
            return Err(AppError::InvalidState("Turn off marks to edit."));
        }
        if self.current_document_is_saving() {
            return Err(AppError::InvalidState("Save is still running."));
        }
        if self
            .app
            .current_document()
            .is_some_and(|document| document.is_read_only())
        {
            return Err(AppError::InvalidState("This is read-only."));
        }
        Ok(())
    }

    fn current_document_is_saving(&self) -> bool {
        let Some(pending) = self.pending_save.as_ref() else {
            return false;
        };
        self.app
            .current_document()
            .is_some_and(|document| document.id() == pending.document_id)
    }

    fn search(&mut self, direction: SearchDirection) -> Result<(), AppError> {
        self.search_with_find_focus_restore(direction, self.find_entry_has_focus())
    }

    fn search_from_find_entry(&mut self, direction: SearchDirection) -> Result<(), AppError> {
        self.search_with_find_focus_restore(direction, true)
    }

    fn search_with_find_focus_restore(
        &mut self,
        direction: SearchDirection,
        restore_find_focus: bool,
    ) -> Result<(), AppError> {
        let query = self.find_entry.text().to_string();
        if query.is_empty() {
            self.show_find_bar_and_focus(false);
            return Ok(());
        }
        self.sync_current_text()?;
        if self.show_search_results && !self.app.search_results().is_empty() {
            if restore_find_focus {
                self.focus_find_entry();
            }
            if let Some(index) = self.app.move_active_search_result(direction) {
                self.select_search_result(index)?;
            }
            return Ok(());
        }
        let (selection_start, selection_end, _) = self.selection_char_offsets();
        let start_byte = self
            .app
            .current_document()
            .map(|document| {
                let char_offset = match direction {
                    SearchDirection::Forward => selection_end,
                    SearchDirection::Backward => selection_start,
                };
                char_offset_to_byte_index(document.content(), char_offset.max(0) as usize)
            })
            .unwrap_or(0);
        let range = self
            .app
            .current_document()
            .and_then(|document| find_text(document.content(), &query, start_byte, direction));
        if let Some(range) = range {
            if restore_find_focus {
                self.focus_find_entry();
            }
            self.select_byte_range(range.start, range.end);
        } else {
            self.show_info(FIND_DIALOG_TITLE, NO_MATCH_DIALOG_MESSAGE);
            if restore_find_focus {
                self.focus_find_entry();
            }
        }
        Ok(())
    }

    fn find_all_results(&mut self) -> Result<(), AppError> {
        let query = self.find_entry.text().to_string();
        if query.is_empty() {
            self.show_find_bar = true;
            self.show_search_results = false;
            self.show_or_hide_find_bar();
            self.show_or_hide_search_results();
            self.find_entry.grab_focus();
            return Ok(());
        }
        self.sync_current_text()?;
        self.app.update_search_results(&query)?;
        self.show_find_bar = true;
        self.show_search_results = true;
        self.populate_search_results();
        self.show_or_hide_find_bar();
        self.show_or_hide_search_results();
        if self.app.search_results().is_empty() {
            self.show_info(RESULTS_DIALOG_TITLE, NO_MATCH_DIALOG_MESSAGE);
        } else {
            self.select_search_result(0)?;
        }
        Ok(())
    }

    fn populate_search_results(&mut self) {
        clear_list_box(&self.search_results_list);
        for result in self.app.search_results() {
            let label = gtk::Label::new(Some(&format!(
                "{}:{}  {}",
                result.line,
                result.column,
                result.preview.trim()
            )));
            label.set_xalign(0.0);
            label.set_margin_start(8);
            label.set_margin_end(8);
            label.set_margin_top(3);
            label.set_margin_bottom(3);
            self.search_results_list.append(&label);
        }
    }

    fn select_search_result(&mut self, index: usize) -> Result<(), AppError> {
        self.app.set_active_search_result(index)?;
        let result = self
            .app
            .search_results()
            .get(index)
            .ok_or(AppError::InvalidState("Result not found."))?;
        self.select_byte_range(result.range.start, result.range.end);
        if let Some(row) = self.search_results_list.row_at_index(index as i32) {
            self.search_results_list.select_row(Some(&row));
        }
        Ok(())
    }

    fn replace_current(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.ensure_current_writable()?;
        let query = self.find_entry.text().to_string();
        if query.is_empty() {
            return Ok(());
        }
        let replacement = self.replace_entry.text().to_string();
        self.sync_current_text()?;
        let selected_matches = match self.app.current_document() {
            Some(document) => {
                let text = document.content();
                let (selection_start, selection_end, _) = self.selection_char_offsets();
                let start_byte = char_offset_to_byte_index(text, selection_start.max(0) as usize);
                let end_byte = char_offset_to_byte_index(text, selection_end.max(0) as usize);
                let selected_matches = text
                    .get(start_byte..end_byte)
                    .is_some_and(|selected| selected == query);
                if selected_matches {
                    validate_selection_replacement_document_size(
                        text,
                        start_byte,
                        end_byte,
                        &replacement,
                    )?;
                }
                selected_matches
            }
            None => false,
        };
        if selected_matches {
            self.replace_selection_with_text(&replacement);
            self.sync_after_text_buffer_command()?;
        }
        self.search(SearchDirection::Forward)
    }

    fn replace_all(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.ensure_current_writable()?;
        let query = self.find_entry.text().to_string();
        if query.is_empty() {
            return Ok(());
        }
        let replacement = self.replace_entry.text().to_string();
        self.sync_current_text()?;
        let Some(document) = self.app.current_document() else {
            return Ok(());
        };
        let Some(replaced) = replace_all_text_if_changed(document.content(), &query, &replacement)?
        else {
            return Ok(());
        };
        self.programmatic_update = true;
        self.buffer.set_text(&replaced.text);
        self.programmatic_update = false;
        self.app
            .update_current_content_with_metrics(replaced.text, replaced.metrics)?;
        self.edit_content_pending_sync = false;
        self.app.clear_search_results();
        self.show_search_results = false;
        self.show_or_hide_search_results();
        self.refresh_current_tab_display();
        self.update_status();
        self.update_line_numbers();
        self.update_menu_checks();
        Ok(())
    }

    fn undo(&mut self) -> Result<(), AppError> {
        self.ensure_current_writable()?;
        if self.buffer.can_undo() {
            self.buffer.undo();
            self.sync_after_text_buffer_command()?;
        }
        Ok(())
    }

    fn redo(&mut self) -> Result<(), AppError> {
        self.ensure_current_writable()?;
        if self.buffer.can_redo() {
            self.buffer.redo();
            self.sync_after_text_buffer_command()?;
        }
        Ok(())
    }

    fn cut(&mut self) -> Result<(), AppError> {
        self.ensure_current_writable()?;
        if let Some((mut start, mut end)) = self.buffer.selection_bounds() {
            let text = self.buffer.text(&start, &end, false);
            let clipboard = gtk::prelude::WidgetExt::display(&self.window).clipboard();
            clipboard.set_text(text.as_str());
            self.buffer.delete(&mut start, &mut end);
        }
        self.sync_after_text_buffer_command()
    }

    fn copy(&self) {
        if let Some((start, end)) = self.buffer.selection_bounds() {
            let text = self.buffer.text(&start, &end, false);
            let clipboard = gtk::prelude::WidgetExt::display(&self.window).clipboard();
            clipboard.set_text(text.as_str());
        }
    }

    fn paste(&mut self) -> Result<(), AppError> {
        self.ensure_current_writable()?;
        let clipboard = gtk::prelude::WidgetExt::display(&self.window).clipboard();
        let Some(pending) = self.prepare_plain_text_paste() else {
            return Ok(());
        };
        if let Some(text) = local_clipboard_text(&clipboard) {
            return self.finish_plain_text_paste(pending, Ok(Some(text)));
        }
        let weak = self.self_weak.clone();
        clipboard.read_text_async(None::<&gio::Cancellable>, move |result| {
            let result = result
                .map(|text| text.map(|text| text.to_string()))
                .map_err(clipboard_text_read_error);
            finish_plain_text_paste_async(weak, pending, result);
        });
        Ok(())
    }

    fn select_all_text(&self) {
        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        self.buffer.select_range(&start, &end);
    }

    fn sync_after_text_buffer_command(&mut self) -> Result<(), AppError> {
        self.edit_content_pending_sync = true;
        self.app.mark_current_dirty_from_view()?;
        self.sync_current_text()?;
        self.app.clear_search_results();
        self.show_search_results = false;
        self.show_or_hide_search_results();
        self.refresh_current_tab_display();
        self.update_status();
        self.update_line_numbers();
        self.update_menu_checks();
        Ok(())
    }

    fn replace_selection_with_text(&mut self, text: &str) {
        if let Some((mut start, mut end)) = self.buffer.selection_bounds() {
            self.buffer.delete(&mut start, &mut end);
            self.buffer.insert(&mut start, text);
        }
    }

    fn prepare_plain_text_paste(&self) -> Option<PendingPlainTextPaste> {
        let document_id = self.app.current_document().map(|document| document.id())?;
        let (start, end) = self.buffer.selection_bounds().unwrap_or_else(|| {
            let insert = self.buffer.get_insert();
            let iter = self.buffer.iter_at_mark(&insert);
            (iter, iter)
        });
        Some(PendingPlainTextPaste {
            document_id,
            start_mark: self.buffer.create_mark(None, &start, true),
            end_mark: self.buffer.create_mark(None, &end, false),
        })
    }

    fn finish_plain_text_paste(
        &mut self,
        pending: PendingPlainTextPaste,
        text: Result<Option<String>, AppError>,
    ) -> Result<(), AppError> {
        let mut start = self.buffer.iter_at_mark(&pending.start_mark);
        let mut end = self.buffer.iter_at_mark(&pending.end_mark);
        self.buffer.delete_mark(&pending.start_mark);
        self.buffer.delete_mark(&pending.end_mark);

        let Some(text) = text? else {
            return Ok(());
        };
        if text.is_empty()
            || self.app.current_document().map(|document| document.id())
                != Some(pending.document_id)
        {
            return Ok(());
        }
        self.ensure_current_writable()?;
        if text.contains('\0') {
            return Err(AppError::encoding_unsafe_text(
                "Clipboard text contains NUL text",
            ));
        }
        if start.offset() > end.offset() {
            std::mem::swap(&mut start, &mut end);
        }
        if start.offset() != end.offset() {
            self.buffer.delete(&mut start, &mut end);
        }
        self.buffer.insert(&mut start, text.as_str());
        self.sync_after_text_buffer_command()
    }

    fn show_find_bar_and_focus(&mut self, replace: bool) {
        self.show_find_bar = true;
        self.show_or_hide_find_bar();
        if replace {
            gtk::prelude::GtkWindowExt::set_focus(&self.window, Some(&self.replace_entry));
            self.replace_entry.grab_focus();
        } else {
            self.focus_find_entry();
        }
    }

    fn focus_find_entry(&self) {
        gtk::prelude::GtkWindowExt::set_focus(&self.window, Some(&self.find_entry));
        self.find_entry.grab_focus();
    }

    fn find_entry_has_focus(&self) -> bool {
        if self.find_entry.has_focus() {
            return true;
        }
        let Some(mut focus) = gtk::prelude::GtkWindowExt::focus(&self.window) else {
            return false;
        };
        let find_entry = self.find_entry.clone().upcast::<gtk::Widget>();
        loop {
            if focus == find_entry {
                return true;
            }
            let Some(parent) = focus.parent() else {
                return false;
            };
            focus = parent;
        }
    }

    fn close_find_bar_and_results(&mut self) {
        self.show_find_bar = false;
        self.show_search_results = false;
        self.show_or_hide_find_bar();
        self.show_or_hide_search_results();
        self.text_view.grab_focus();
    }

    fn show_or_hide_find_bar(&self) {
        self.find_bar.set_visible(self.show_find_bar);
    }

    fn show_or_hide_search_results(&self) {
        self.search_results_panel
            .set_visible(self.show_find_bar && self.show_search_results);
    }

    fn show_or_hide_command_palette(&self) {
        self.command_palette.set_visible(self.show_command_palette);
    }

    fn toggle_command_palette(&mut self) -> Result<(), AppError> {
        self.show_command_palette = !self.show_command_palette;
        self.show_or_hide_command_palette();
        if self.show_command_palette {
            self.command_filter.grab_focus();
            self.update_command_palette_filter()?;
        } else {
            self.text_view.grab_focus();
        }
        self.update_menu_checks();
        Ok(())
    }

    fn update_command_palette_filter(&mut self) -> Result<(), AppError> {
        let filter = self.command_filter.text().to_ascii_lowercase();
        self.filtered_command_ids.clear();
        clear_list_box(&self.command_list);
        for command in &self.command_items {
            let label = command_label(*command);
            if !filter.is_empty() && !label.to_ascii_lowercase().contains(&filter) {
                continue;
            }
            let row_label = gtk::Label::new(Some(&label));
            row_label.set_xalign(0.0);
            row_label.set_margin_start(8);
            row_label.set_margin_end(8);
            row_label.set_margin_top(3);
            row_label.set_margin_bottom(3);
            self.command_list.append(&row_label);
            self.filtered_command_ids.push(command.id);
        }
        if let Some(row) = self.command_list.row_at_index(0) {
            self.command_list.select_row(Some(&row));
        }
        Ok(())
    }

    fn activate_selected_command(&mut self) -> Result<(), AppError> {
        let Some(row) = self.command_list.selected_row() else {
            return Ok(());
        };
        self.activate_command_row(row.index() as usize)
    }

    fn activate_command_row(&mut self, row_index: usize) -> Result<(), AppError> {
        let Some(command_id) = self.filtered_command_ids.get(row_index).copied() else {
            return Ok(());
        };
        self.show_command_palette = false;
        self.show_or_hide_command_palette();
        self.execute_editor_command(command_id)
    }

    fn execute_editor_command(&mut self, command_id: EditorCommandId) -> Result<(), AppError> {
        match command_id {
            EditorCommandId::NewFile => self.new_tab(),
            EditorCommandId::OpenFile => self.open_file(None),
            EditorCommandId::Save => self.save_current_command(false),
            EditorCommandId::SaveAs => self.save_current_command(true),
            EditorCommandId::CloseTab => self.close_current_tab(),
            EditorCommandId::CloseOtherTabs => self.close_other_tabs(),
            EditorCommandId::Undo => self.undo(),
            EditorCommandId::Redo => self.redo(),
            EditorCommandId::Cut => self.cut(),
            EditorCommandId::Copy => {
                self.copy();
                Ok(())
            }
            EditorCommandId::Paste => self.paste(),
            EditorCommandId::SelectAll => {
                self.select_all_text();
                Ok(())
            }
            EditorCommandId::Find => {
                self.show_find_bar_and_focus(false);
                Ok(())
            }
            EditorCommandId::Replace => {
                self.show_find_bar_and_focus(true);
                Ok(())
            }
            EditorCommandId::FindAll => self.find_all_results(),
            EditorCommandId::FindNext => self.search(SearchDirection::Forward),
            EditorCommandId::FindPrevious => self.search(SearchDirection::Backward),
            EditorCommandId::CommandPalette => self.toggle_command_palette(),
            EditorCommandId::ToggleLineNumbers => {
                self.show_line_numbers = !self.show_line_numbers;
                self.apply_line_numbers_visibility();
                self.update_line_numbers();
                self.update_menu_checks();
                Ok(())
            }
            EditorCommandId::ToggleVisibleWhitespace => self.toggle_visible_whitespace(),
            EditorCommandId::ToggleWordWrap => self.toggle_word_wrap(),
            EditorCommandId::ReopenWithEncoding => self.reopen_current_file_with_encoding_dialog(),
            EditorCommandId::ConvertEncoding => self.convert_current_encoding_dialog(),
            EditorCommandId::SetLineEnding(line_ending) => self.set_line_ending(line_ending),
        }
    }

    #[allow(deprecated)]
    fn choose_font(&mut self) -> Result<(), AppError> {
        if env::var_os(ACTION_SMOKE_ENV).is_some() {
            let mut settings = self.app.settings().clone();
            settings.font_name = "Monospace".to_string();
            settings.font_size_pt = 13;
            self.apply_settings(settings);
            return Ok(());
        }

        let settings = self.app.settings();
        let font = format!("{} {}", settings.font_name, settings.font_size_pt);
        let dialog = gtk::FontChooserDialog::builder()
            .title("Font")
            .transient_for(&self.window)
            .modal(true)
            .font(font)
            .build();
        let response = run_modal_future(dialog.run_future());
        let result = if response == gtk::ResponseType::Ok {
            dialog.font_desc().and_then(|description| {
                let family = description.family()?.to_string();
                let size =
                    font_size_points_from_pango_size(description.size(), settings.font_size_pt);
                Some((family, size))
            })
        } else {
            None
        };
        dialog.close();
        if let Some((font_name, font_size_pt)) = result {
            let mut settings = self.app.settings().clone();
            settings.font_name = font_name;
            settings.font_size_pt = font_size_pt;
            self.apply_settings(settings);
        }
        Ok(())
    }

    fn set_tab_size(&mut self, tab_size: u8) -> Result<(), AppError> {
        let mut settings = self.app.settings().clone();
        settings.tab_size = tab_size;
        self.apply_settings(settings);
        Ok(())
    }

    fn toggle_word_wrap(&mut self) -> Result<(), AppError> {
        let mut settings = self.app.settings().clone();
        settings.word_wrap = !settings.word_wrap;
        self.apply_settings(settings);
        Ok(())
    }

    fn toggle_visible_whitespace(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        let mut settings = self.app.settings().clone();
        if !settings.show_whitespace
            && let Some(byte_len) = self
                .app
                .current_document()
                .map(|document| document.content().len())
                .filter(|byte_len| !can_render_visible_whitespace_bytes(*byte_len))
        {
            self.show_info("Marks", &visible_whitespace_size_limit_message(byte_len));
            return Ok(());
        }
        settings.show_whitespace = !settings.show_whitespace;
        self.apply_settings(settings);
        self.load_current_document_into_edit()
    }

    fn set_theme(&mut self, theme: ThemeMode) -> Result<(), AppError> {
        let mut settings = self.app.settings().clone();
        settings.theme = theme;
        self.apply_settings(settings);
        Ok(())
    }

    fn apply_settings(&mut self, settings: EditorSettings) {
        self.app.set_settings(settings);
        self.apply_settings_to_view();
        self.rebuild_menu_later();
        self.persist_settings_report_errors();
        self.update_status();
        self.update_menu_checks();
        self.update_accelerators();
    }

    fn apply_settings_to_view(&mut self) {
        let settings = self.app.settings();
        self.text_view.set_wrap_mode(if settings.word_wrap {
            gtk::WrapMode::WordChar
        } else {
            gtk::WrapMode::None
        });
        let mut tab_array = pango::TabArray::new(1, true);
        tab_array.set_tab(
            0,
            pango::TabAlign::Left,
            self.editor_tab_width_pixels(settings),
        );
        self.text_view.set_tabs(&tab_array);
        self.line_numbers.set_tabs(&tab_array);
        self.apply_css();
        self.apply_current_read_only();
        self.apply_line_numbers_visibility();
    }

    fn editor_tab_width_pixels(&self, settings: &EditorSettings) -> i32 {
        let font_description = editor_font_description(settings);
        let metrics = self
            .text_view
            .pango_context()
            .metrics(Some(&font_description), None);
        tab_width_pixels(settings.tab_size, metrics.approximate_char_width())
    }

    fn resolved_theme(&self) -> ThemeMode {
        resolve_theme_mode(self.app.settings().theme, system_prefers_dark_theme())
    }

    fn apply_system_theme_change(&self) {
        if self.app.settings().theme == ThemeMode::System {
            self.apply_css();
        }
    }

    #[allow(deprecated)]
    fn apply_css(&self) {
        let settings = self.app.settings();
        let palette = ThemePalette::for_theme(self.resolved_theme());
        let css = format!(
            "
            window {{
                background: {background};
                color: {foreground};
            }}
            textview.editor, textview.editor text {{
                font-family: '{font}';
                font-size: {size}pt;
                background: {editor_bg};
                color: {foreground};
            }}
            textview.line-numbers, textview.line-numbers text {{
                font-family: '{font}';
                font-size: {size}pt;
                background: {panel_bg};
                color: {muted};
            }}
            entry, listbox, listbox row {{
                font-family: '{font}';
                font-size: {size}pt;
            }}
            .status {{
                background: {panel_bg};
                color: {foreground};
                border-top: 1px solid {border};
            }}
            entry, listbox, notebook, popover, menubar {{
                background: {background};
                color: {foreground};
            }}
            ",
            background = palette.background,
            editor_bg = palette.editor_background,
            panel_bg = palette.panel_background,
            foreground = palette.foreground,
            muted = palette.muted,
            border = palette.border,
            font = settings.font_name.replace('\'', ""),
            size = settings.font_size_pt,
        );
        self.css_provider.load_from_data(&css);
    }

    fn apply_current_read_only(&self) {
        let status = self.app.current_editor_status();
        self.text_view.set_editable(status.can_edit);
        self.text_view
            .set_cursor_visible(status.document_id.is_some());
    }

    fn apply_line_numbers_visibility(&self) {
        self.line_numbers_scrolled
            .set_visible(self.show_line_numbers);
    }

    fn update_accelerators(&self) {
        let shortcuts = &self.app.settings().shortcuts;
        for command in EditorCommandId::SHORTCUT_COMMANDS {
            let action = action_name_for_command(command);
            let accels = if editor_control_shortcut_command(command) {
                Vec::new()
            } else {
                shortcuts
                    .shortcut_for(command)
                    .map(shortcut_to_gtk_accel)
                    .into_iter()
                    .collect::<Vec<_>>()
            };
            let accel_refs = accels.iter().map(String::as_str).collect::<Vec<_>>();
            self.application
                .set_accels_for_action(action, accel_refs.as_slice());
        }
    }

    fn update_status(&mut self) {
        self.refresh_current_editor_surface_state();
        let status = self.app.current_editor_status();
        let selected_chars = self.selected_char_count();
        self.status
            .line
            .set_text(&format!("Line {}, Col {}", status.line, status.column));
        self.status
            .chars
            .set_text(&format!("Chars {}", status.char_count));
        self.status
            .selected
            .set_text(&format!("Selected {}", selected_chars));
        self.status
            .encoding
            .set_text(status.encoding.display_name());
        self.status
            .line_ending
            .set_text(status.line_ending.display_name());
        self.status.wrap.set_text(if status.word_wrap {
            "Wrap On"
        } else {
            "Wrap Off"
        });
        self.status
            .save_state
            .set_text(editor_save_state_label(&status));
        self.status
            .detail
            .set_text(&editor_status_state_text(&status));
        let title = editor_window_title(&status);
        self.window.set_title(Some(&title));
    }

    fn update_line_numbers(&mut self) {
        if !self.show_line_numbers {
            self.line_numbers_snapshot = None;
            return;
        }
        let line_count = self.buffer.line_count().max(1) as usize;
        let first_line = self.first_visible_line();
        let visible_count = 200usize.min(line_count.saturating_sub(first_line).saturating_add(1));
        let snapshot = LineNumbersSnapshot {
            first_line,
            visible_count,
        };
        if self.line_numbers_snapshot == Some(snapshot) {
            return;
        }

        let last_visible_line = first_line
            .saturating_add(visible_count)
            .min(line_count)
            .max(1);
        let mut max_label = last_visible_line;
        let mut label_digits = 1usize;
        while max_label >= 10 {
            max_label /= 10;
            label_digits += 1;
        }
        let mut text = String::with_capacity(visible_count.saturating_mul(label_digits + 2));
        for line in first_line..first_line.saturating_add(visible_count) {
            if line >= line_count {
                break;
            }
            let _ = writeln!(&mut text, "{}", line + 1);
        }
        self.line_numbers_buffer.set_text(&text);
        self.line_numbers_snapshot = Some(snapshot);
    }

    fn first_visible_line(&self) -> usize {
        let adjustment = self.editor_scrolled.vadjustment();
        let y = adjustment.value();
        let (iter, _) = self.text_view.line_at_y(y as i32);
        iter.line().max(0) as usize
    }

    fn update_menu_checks(&mut self) {
        let status = self.app.current_editor_status();
        self.set_action_enabled("save", status.can_save);
        self.set_action_enabled("save-as", status.can_save_as);
        self.set_action_enabled("close-tab", status.document_id.is_some());
        self.set_action_enabled("close-other-tabs", status.document_id.is_some());
        self.set_action_enabled("close-all-tabs", status.document_id.is_some());
        self.set_action_enabled("undo", status.can_undo);
        self.set_action_enabled("redo", status.can_redo);
        self.set_action_enabled("cut", status.can_edit);
        self.set_action_enabled("copy", status.document_id.is_some());
        self.set_action_enabled("paste", status.can_edit);
        self.set_action_enabled("select-all", status.document_id.is_some());
        self.set_action_enabled("find", status.document_id.is_some());
        self.set_action_enabled("replace", status.document_id.is_some());
        self.set_action_enabled("find-next", status.document_id.is_some());
        self.set_action_enabled("find-previous", status.document_id.is_some());
        self.set_action_enabled("find-all", status.document_id.is_some());
        self.set_action_enabled("replace-current", status.document_id.is_some());
        self.set_action_enabled("replace-all", status.document_id.is_some());
        self.set_action_enabled("reopen-encoding", status.document_id.is_some());
        self.set_action_enabled("change-encoding", status.can_edit);
        self.set_action_enabled("line-ending-crlf", status.can_edit);
        self.set_action_enabled("line-ending-lf", status.can_edit);
        self.set_action_enabled("line-ending-cr", status.can_edit);
        self.set_action_enabled("tab-left", status.document_id.is_some());
        self.set_action_enabled("tab-right", status.document_id.is_some());
        self.set_action_enabled("open-new-window", self.app.document_count() > 1);

        let settings = self.app.settings();
        self.set_action_bool_state("command-palette", self.show_command_palette);
        self.set_action_bool_state("line-numbers", self.show_line_numbers);
        self.set_action_bool_state("visible-whitespace", settings.show_whitespace);
        self.set_action_bool_state("word-wrap", settings.word_wrap);
        self.set_action_bool_state(
            "line-ending-crlf",
            status.document_id.is_some() && status.line_ending == LineEnding::Crlf,
        );
        self.set_action_bool_state(
            "line-ending-lf",
            status.document_id.is_some() && status.line_ending == LineEnding::Lf,
        );
        self.set_action_bool_state(
            "line-ending-cr",
            status.document_id.is_some() && status.line_ending == LineEnding::Cr,
        );
        self.set_action_bool_state("tab-size-2", settings.tab_size == 2);
        self.set_action_bool_state("tab-size-4", settings.tab_size == 4);
        self.set_action_bool_state("tab-size-8", settings.tab_size == 8);
        for theme in ThemeMode::options() {
            self.set_action_bool_state(theme_action_name(*theme), settings.theme == *theme);
        }
        for (index, command) in EditorCommandId::SHORTCUT_COMMANDS
            .iter()
            .copied()
            .enumerate()
        {
            self.set_action_bool_state(
                &format!("shortcut-{index}-default"),
                settings.shortcuts.shortcut_for(command) == command.default_shortcut(),
            );
            self.set_action_bool_state(
                &format!("shortcut-{index}-disable"),
                settings.shortcuts.shortcut_for(command).is_none(),
            );
        }
    }

    fn set_action_enabled(&self, name: &str, enabled: bool) {
        if let Some(action) = self.actions.get(name) {
            action.set_enabled(enabled);
        }
    }

    fn set_action_bool_state(&self, name: &str, active: bool) {
        if let Some(action) = self.actions.get(name) {
            action.set_state(&active.to_variant());
        }
    }

    fn refresh_current_editor_surface_state(&mut self) {
        let document_id = self.app.current_document().map(|document| document.id());
        let (selection_start_chars, selection_end_chars, _) = self.selection_char_offsets();
        let (selection_start_utf16, selection_end_utf16, line, column) = self
            .app
            .current_document()
            .map(|document| {
                let text = document.content();
                let selection_start_chars = selection_start_chars.max(0) as usize;
                (
                    char_offset_to_utf16_offset(text, selection_start_chars),
                    char_offset_to_utf16_offset(text, selection_end_chars.max(0) as usize),
                    line_and_utf16_column_from_char_offset(text, selection_start_chars),
                )
            })
            .map(|(selection_start, selection_end, (line, column))| {
                (selection_start, selection_end, line, column)
            })
            .unwrap_or((0, 0, 1, 1));
        let state = EditorSurfaceState::from_surface(
            document_id,
            selection_start_utf16.min(u32::MAX as usize) as u32,
            selection_end_utf16.min(u32::MAX as usize) as u32,
            line,
            column,
            self.buffer.can_undo(),
            self.buffer.can_redo(),
        );
        self.app.update_current_editor_surface_state(state);
    }

    fn selection_char_offsets(&self) -> (i32, i32, bool) {
        if let Some((start, end)) = self.buffer.selection_bounds() {
            return (start.offset(), end.offset(), true);
        }
        let offset = self.buffer.cursor_position();
        (offset, offset, false)
    }

    fn selected_char_count(&self) -> usize {
        let (start, end, _) = self.selection_char_offsets();
        start.abs_diff(end) as usize
    }

    fn select_byte_range(&mut self, start_byte: usize, end_byte: usize) {
        if let Some(document) = self.app.current_document() {
            let text = document.content();
            let start = byte_index_to_char_offset(text, start_byte);
            let end = byte_index_to_char_offset(text, end_byte);
            let start_iter = self.buffer.iter_at_offset(start);
            let end_iter = self.buffer.iter_at_offset(end);
            self.buffer.select_range(&start_iter, &end_iter);
            let mut scroll_iter = self.buffer.iter_at_offset(start);
            self.text_view
                .scroll_to_iter(&mut scroll_iter, 0.1, false, 0.0, 0.0);
        }
    }

    fn buffer_text(&self) -> String {
        self.buffer
            .text(&self.buffer.start_iter(), &self.buffer.end_iter(), true)
            .to_string()
    }

    #[allow(deprecated)]
    fn open_file_dialog(&self) -> Result<Option<PathBuf>, AppError> {
        if env::var_os(ACTION_SMOKE_ENV).is_some() {
            let path = env::var_os(ACTION_SMOKE_OPEN_PATH_ENV)
                .map(PathBuf::from)
                .ok_or(AppError::InvalidState(
                    "Linux action smoke open path is missing.",
                ))?;
            return Ok(Some(path));
        }

        let dialog = gtk::FileChooserNative::new(
            Some("Open"),
            Some(&self.window),
            gtk::FileChooserAction::Open,
            Some("Open"),
            Some("Cancel"),
        );
        configure_file_dialog_filters(&dialog);
        let response = run_modal_future(dialog.run_future());
        let path = if response == gtk::ResponseType::Accept {
            match dialog.file().and_then(|file| file.path()) {
                Some(path) => Some(path),
                None => {
                    dialog.destroy();
                    return Err(AppError::InvalidState("Selected file is not a local path."));
                }
            }
        } else {
            None
        };
        dialog.destroy();
        Ok(path)
    }

    #[allow(deprecated)]
    fn save_file_dialog(&self) -> Result<Option<PathBuf>, AppError> {
        if env::var_os(ACTION_SMOKE_ENV).is_some() {
            let path = env::var_os(ACTION_SMOKE_SAVE_AS_PATH_ENV)
                .map(PathBuf::from)
                .ok_or(AppError::InvalidState(
                    "Linux action smoke save-as path is missing.",
                ))?;
            return Ok(Some(path));
        }

        let dialog = gtk::FileChooserNative::new(
            Some("Save As"),
            Some(&self.window),
            gtk::FileChooserAction::Save,
            Some("Save"),
            Some("Cancel"),
        );
        configure_file_dialog_filters(&dialog);
        // Match Windows: its OPENFILENAME buffer starts empty, so Save As does
        // not prefill the current document name.
        let response = run_modal_future(dialog.run_future());
        let path = if response == gtk::ResponseType::Accept {
            match dialog.file().and_then(|file| file.path()) {
                Some(path) => Some(path),
                None => {
                    dialog.destroy();
                    return Err(AppError::InvalidState("Selected file is not a local path."));
                }
            }
        } else {
            None
        };
        dialog.destroy();
        Ok(path)
    }

    #[allow(deprecated)]
    fn choose_encoding_dialog(
        &self,
        title: &str,
        heading: &str,
        detail: &str,
        current: TextEncoding,
    ) -> Result<Option<TextEncoding>, AppError> {
        if env::var_os(ACTION_SMOKE_ENV).is_some() {
            return Ok(Some(match title {
                "Reopen" => TextEncoding::Utf8,
                "Save" | "Change Encoding" => TextEncoding::Utf8Bom,
                _ => current,
            }));
        }

        let dialog = gtk::Dialog::builder()
            .title(title)
            .transient_for(&self.window)
            .modal(true)
            .build();
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("OK", gtk::ResponseType::Ok);
        dialog.set_default_response(gtk::ResponseType::Ok);
        let content = dialog.content_area();
        content.set_spacing(8);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        let heading_label = gtk::Label::new(Some(heading));
        heading_label.set_xalign(0.0);
        let detail_label = gtk::Label::new(Some(detail));
        detail_label.set_xalign(0.0);
        let entry = gtk::Entry::new();
        entry.set_text(current.display_name());
        entry.set_activates_default(true);
        let status_label = gtk::Label::new(None);
        status_label.set_xalign(0.0);
        let list = gtk::ListBox::new();
        list.set_activate_on_single_click(true);
        for encoding in TextEncoding::ALL {
            let label = gtk::Label::new(Some(encoding.display_name()));
            label.set_xalign(0.0);
            label.set_margin_start(8);
            label.set_margin_end(8);
            label.set_margin_top(3);
            label.set_margin_bottom(3);
            list.append(&label);
        }
        {
            let entry = entry.clone();
            list.connect_row_activated(move |_, row| {
                if let Some(child) = row.child()
                    && let Ok(label) = child.downcast::<gtk::Label>()
                {
                    entry.set_text(label.text().as_str());
                }
            });
        }
        {
            let status_label = status_label.clone();
            entry.connect_changed(move |entry| {
                update_encoding_status_label(entry, &status_label);
            });
        }
        content.append(&heading_label);
        content.append(&detail_label);
        content.append(&entry);
        content.append(&status_label);
        content.append(&list);
        update_encoding_status_label(&entry, &status_label);
        loop {
            let response = run_modal_future(dialog.run_future());
            if response != gtk::ResponseType::Ok {
                dialog.close();
                return Ok(None);
            }
            let text = entry.text();
            if let Some(encoding) = encoding_from_dialog_input(text.as_str()) {
                dialog.close();
                return Ok(Some(encoding));
            }
            self.show_warning("Encoding", &invalid_encoding_message(text.as_str()));
            update_encoding_status_label(&entry, &status_label);
            entry.grab_focus();
        }
    }

    fn ask_yes_no(&self, title: &str, message: &str) -> DialogChoice {
        if env::var_os(ACTION_SMOKE_ENV).is_some()
            && let Some(choice) = self.yes_no_prompt_smoke_decision.take()
        {
            return choice;
        }
        run_choice_dialog(
            Some(&self.window),
            title,
            message,
            gtk::MessageType::Warning,
            YES_NO_BUTTONS,
        )
    }

    fn ask_save_discard_cancel(&self, title: &str, message: &str) -> DialogChoice {
        if env::var_os(ACTION_SMOKE_ENV).is_some()
            && let Some(choice) = self.dirty_prompt_smoke_decision.take()
        {
            return choice;
        }
        run_choice_dialog(
            Some(&self.window),
            title,
            message,
            gtk::MessageType::Warning,
            YES_NO_CANCEL_BUTTONS,
        )
    }

    fn ask_external_file_changed_action(&self, path: &Path) -> ExternalFileChangedAction {
        let message = format!(
            "{}\n\nReload discards this tab's current text and reads the file from disk.\nSave As keeps this tab's current text and saves it to another file.",
            path.display()
        );
        if env::var_os(ACTION_SMOKE_ENV).is_some() {
            eprintln!("Linux action smoke external change: {message}");
            return ExternalFileChangedAction::Cancel;
        }
        run_external_file_changed_dialog(Some(&self.window), &message)
    }

    fn confirm_save_as_overwrite(&self, path: &Path, expectation: SaveTargetExpectation) -> bool {
        if !save_target_needs_overwrite_confirmation(expectation) {
            return true;
        }
        let message = save_as_overwrite_message(path);
        matches!(
            self.ask_yes_no(SAVE_AS_OVERWRITE_DIALOG_TITLE, &message),
            DialogChoice::Yes
        )
    }

    fn show_error(&self, error: &AppError) {
        show_error_dialog(Some(&self.window), error);
    }

    fn show_warning(&self, title: &str, message: &str) {
        if env::var_os(ACTION_SMOKE_ENV).is_some() {
            eprintln!("Linux action smoke warning: {title}: {message}");
            return;
        }
        show_message_dialog(
            Some(&self.window),
            title,
            message,
            gtk::MessageType::Warning,
        );
    }

    fn show_info(&self, title: &str, message: &str) {
        if env::var_os(ACTION_SMOKE_ENV).is_some() {
            eprintln!("Linux action smoke info: {title}: {message}");
            return;
        }
        show_message_dialog(Some(&self.window), title, message, gtk::MessageType::Info);
    }

    fn show_about(&self) {
        if env::var_os(ACTION_SMOKE_ENV).is_some() {
            eprintln!(
                "Linux action smoke info: {ABOUT_DIALOG_TITLE}: {ABOUT_DIALOG_MESSAGE}\n{ABOUT_DIALOG_URL}"
            );
            return;
        }
        show_about_dialog(Some(&self.window));
    }

    fn show_startup_warnings(&mut self) {
        if self.startup_warnings.is_empty() {
            return;
        }
        let message = self.startup_warnings.join("\n");
        self.startup_warnings.clear();
        self.show_warning("Startup", &message);
    }

    fn handle_drop_paths(&mut self, paths: Vec<PathBuf>) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        if paths.is_empty() {
            return Ok(());
        }
        for path in paths {
            self.open_path_as_new_tab(path, None)?;
        }
        Ok(())
    }

    fn move_current_tab_left(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        if self.app.move_current_tab_left() {
            self.refresh_tabs();
            self.load_current_document_into_edit()?;
        }
        Ok(())
    }

    fn move_current_tab_right(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        if self.app.move_current_tab_right() {
            self.refresh_tabs();
            self.load_current_document_into_edit()?;
        }
        Ok(())
    }

    fn persist_settings_report_errors(&mut self) {
        self.pending_persistence.request_settings();
    }

    fn persist_recent_files_report_errors(&mut self) {
        self.pending_persistence.request_recent_files();
    }

    fn flush_ready_persistence_report_errors(&mut self) {
        if self.pending_persistence.tick_elapsed() {
            self.flush_pending_persistence_report_errors();
        }
    }

    fn flush_pending_persistence_report_errors(&mut self) {
        let Some(store) = self.store.as_ref() else {
            self.pending_persistence = PendingPersistence::default();
            return;
        };
        if !self.pending_persistence.has_pending() {
            return;
        }
        let pending = self.pending_persistence.take_pending();
        let mut errors = Vec::new();
        if pending.settings
            && let Err(error) = store.save_settings(self.app.settings())
        {
            errors.push(error.user_message());
        }
        if pending.recent_files
            && let Err(error) = store.save_recent_files(self.app.recent_files())
        {
            errors.push(error.user_message());
        }
        if errors.is_empty() {
            self.last_persist_error = None;
        } else {
            let message = errors.join("\n");
            if self.last_persist_error.as_deref() != Some(message.as_str()) {
                self.show_warning("Settings", &message);
                self.last_persist_error = Some(message);
            }
        }
    }

    fn configure_shortcut(
        &mut self,
        command: EditorCommandId,
        action: ShortcutMenuAction,
    ) -> Result<(), AppError> {
        let shortcut = match action {
            ShortcutMenuAction::Capture => {
                let Some(shortcut) = self.capture_shortcut(command)? else {
                    return Ok(());
                };
                Some(shortcut)
            }
            ShortcutMenuAction::UseDefault => command.default_shortcut(),
            ShortcutMenuAction::Disable => None,
        };
        let mut settings = self.app.settings().clone();
        if let Some(shortcut) = shortcut
            && let Some(existing) = settings
                .shortcuts
                .command_for(shortcut)
                .filter(|existing| *existing != command)
        {
            let existing_title = existing.shortcut_title().unwrap_or("another command");
            let message = format!(
                "{} is used by {}.\n\nUse it here?",
                shortcut.display_name(),
                existing_title
            );
            if !matches!(self.ask_yes_no("Shortcut", &message), DialogChoice::Yes) {
                return Ok(());
            }
            settings
                .shortcuts
                .clear_matching_shortcut(shortcut, command);
        }
        settings.shortcuts.set_shortcut(command, shortcut);
        self.apply_settings(settings);
        Ok(())
    }

    #[allow(deprecated)]
    fn capture_shortcut(
        &self,
        command: EditorCommandId,
    ) -> Result<Option<KeyboardShortcut>, AppError> {
        if env::var_os(ACTION_SMOKE_ENV).is_some() {
            return Ok(Some(KeyboardShortcut::CTRL_F4));
        }

        let captured = Rc::new(RefCell::new(None));
        let dialog = gtk::Dialog::builder()
            .title("Set Shortcut")
            .transient_for(&self.window)
            .modal(true)
            .build();
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        let content = dialog.content_area();
        content.set_spacing(8);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        let label = gtk::Label::new(Some(&format!(
            "Press keys for {}.\nUse Ctrl, Alt, or an F-key. Esc cancels.",
            command.shortcut_title().unwrap_or("Command")
        )));
        label.set_xalign(0.0);
        content.append(&label);

        let key_controller = gtk::EventControllerKey::new();
        {
            let captured = Rc::clone(&captured);
            let dialog = dialog.clone();
            key_controller.connect_key_pressed(move |_, key, _, state| {
                if key == gdk::Key::Escape {
                    dialog.response(gtk::ResponseType::Cancel);
                    return glib::Propagation::Stop;
                }
                if let Some(shortcut) = shortcut_from_key_event(key, state) {
                    *captured.borrow_mut() = Some(shortcut);
                    dialog.response(gtk::ResponseType::Ok);
                    return glib::Propagation::Stop;
                }
                if shortcut_key_from_gdk_key(key).is_some() {
                    show_message_dialog_for_window(
                        Some(dialog.upcast_ref::<gtk::Window>()),
                        "Shortcut",
                        "Use Ctrl, Alt, or an F-key.",
                        gtk::MessageType::Warning,
                    );
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
        }
        dialog.add_controller(key_controller);
        let response = run_modal_future(dialog.run_future());
        let result = if response == gtk::ResponseType::Ok {
            *captured.borrow()
        } else {
            None
        };
        dialog.close();
        Ok(result)
    }
}

#[derive(Clone, Copy)]
struct ConfirmedLargeFilePolicy {
    read_only_reason: Option<ReadOnlyReason>,
    byte_len: u64,
}

#[derive(Clone, Copy)]
enum ShortcutMenuAction {
    Capture,
    UseDefault,
    Disable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DialogChoice {
    Yes,
    No,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExternalFileChangedAction {
    Reload,
    SaveAs,
    Cancel,
}

#[derive(Clone, Copy)]
struct ThemePalette {
    background: &'static str,
    editor_background: &'static str,
    panel_background: &'static str,
    foreground: &'static str,
    muted: &'static str,
    border: &'static str,
}

impl ThemePalette {
    fn for_theme(theme: ThemeMode) -> Self {
        match theme {
            ThemeMode::System | ThemeMode::Light => Self {
                background: "#f0f0f0",
                editor_background: "#ffffff",
                panel_background: "#f0f0f0",
                foreground: "#000000",
                muted: "#606060",
                border: "#d0d0d0",
            },
            ThemeMode::ClassicDark => Self {
                background: "#1f2124",
                editor_background: "#181a1d",
                panel_background: "#1f2124",
                foreground: "#e6e8eb",
                muted: "#5c6169",
                border: "#2f3338",
            },
            ThemeMode::SepiaTeal => Self {
                background: "#181918",
                editor_background: "#1f3438",
                panel_background: "#181918",
                foreground: "#ece8db",
                muted: "#b29a7c",
                border: "#31464a",
            },
            ThemeMode::Graphite => Self {
                background: "#18191a",
                editor_background: "#32373f",
                panel_background: "#18191a",
                foreground: "#efece5",
                muted: "#7e7769",
                border: "#444a53",
            },
            ThemeMode::Forest => Self {
                background: "#161917",
                editor_background: "#273b3f",
                panel_background: "#161917",
                foreground: "#ecefe5",
                muted: "#689675",
                border: "#385448",
            },
            ThemeMode::SteelBlue => Self {
                background: "#18191b",
                editor_background: "#364050",
                panel_background: "#18191b",
                foreground: "#eff0f2",
                muted: "#688bab",
                border: "#46546a",
            },
        }
    }
}

struct ReplaceAllText {
    text: String,
    metrics: crate::domain::DocumentMetrics,
}

fn add_window_action<F>(this: &Rc<RefCell<MainWindow>>, name: &'static str, f: F)
where
    F: Fn(&mut MainWindow) -> Result<(), AppError> + 'static,
{
    let action = gio::SimpleAction::new(name, None);
    let weak = Rc::downgrade(this);
    action.connect_activate(move |_, _| {
        with_window_report_errors(&weak, |window| f(window));
    });
    {
        let window = this.borrow().window.clone();
        window.add_action(&action);
    }
    this.borrow_mut().actions.insert(name, action);
}

fn add_window_toggle_action<F>(
    this: &Rc<RefCell<MainWindow>>,
    name: &'static str,
    active: bool,
    f: F,
) where
    F: Fn(&mut MainWindow) -> Result<(), AppError> + 'static,
{
    let action = gio::SimpleAction::new_stateful(name, None, &active.to_variant());
    let weak = Rc::downgrade(this);
    action.connect_activate(move |_, _| {
        with_window_report_errors(&weak, |window| f(window));
    });
    {
        let window = this.borrow().window.clone();
        window.add_action(&action);
    }
    this.borrow_mut().actions.insert(name, action);
}

fn with_window_report_errors<F>(weak: &Weak<RefCell<MainWindow>>, f: F)
where
    F: FnOnce(&mut MainWindow) -> Result<(), AppError>,
{
    let Some(this) = weak.upgrade() else {
        return;
    };
    let result = {
        let Ok(mut window) = this.try_borrow_mut() else {
            return;
        };
        f(&mut window)
    };
    if let Err(error) = result {
        if let Ok(mut window) = this.try_borrow_mut() {
            window.report_error(error);
        } else {
            show_error_dialog(None, &error);
        }
    }
}

fn finish_plain_text_paste_async(
    weak: Weak<RefCell<MainWindow>>,
    pending: PendingPlainTextPaste,
    result: Result<Option<String>, AppError>,
) {
    let Some(this) = weak.upgrade() else {
        return;
    };
    let result = {
        let Ok(mut window) = this.try_borrow_mut() else {
            glib::idle_add_local_once(move || {
                finish_plain_text_paste_async(weak, pending, result);
            });
            return;
        };
        window.finish_plain_text_paste(pending, result)
    };
    if let Err(error) = result {
        if let Ok(window) = this.try_borrow() {
            show_error_dialog(Some(&window.window), &error);
        } else {
            show_error_dialog(None, &error);
        }
    }
}

fn local_clipboard_text(clipboard: &gdk::Clipboard) -> Option<String> {
    if !clipboard.is_local() {
        return None;
    }
    clipboard
        .content()?
        .value(String::static_type())
        .ok()?
        .get::<String>()
        .ok()
}

fn clipboard_text_read_error(error: glib::Error) -> AppError {
    eprintln!("Clipboard text read failed: {error}");
    AppError::InvalidState("Clipboard text could not be read.")
}

fn write_action_smoke_report(label: &str, result: Result<&Vec<String>, &String>) {
    let text = match result {
        Ok(steps) => format!(
            "PASS: {label} completed\nsteps={}\n{}\n",
            steps.len(),
            steps.join("\n")
        ),
        Err(error) => format!("FAIL: {label} failed\n{error}\n"),
    };
    if let Some(path) = env::var_os(ACTION_SMOKE_REPORT_ENV) {
        if let Err(error) = fs::write(&path, text.as_bytes()) {
            eprintln!(
                "failed to write Linux action smoke report to {}: {error}",
                PathBuf::from(path).display()
            );
        }
    } else {
        eprint!("{text}");
    }
}

fn smoke_activate_action(
    this: &Rc<RefCell<MainWindow>>,
    name: &str,
    steps: &mut Vec<String>,
) -> Result<(), String> {
    let action = {
        let window = this
            .try_borrow()
            .map_err(|_| format!("window state was busy before activating {name}"))?;
        window
            .actions
            .get(name)
            .cloned()
            .ok_or_else(|| format!("action {name} was not registered"))?
    };
    if !action.is_enabled() {
        return Err(format!("action {name} was disabled"));
    }
    if env::var_os(ACTION_SMOKE_ENV).is_some() {
        eprintln!("Linux action smoke activating {name}");
    }
    action.activate(None);
    steps.push(name.to_string());
    Ok(())
}

fn smoke_require_action_state(
    this: &Rc<RefCell<MainWindow>>,
    name: &str,
    active: bool,
    steps: &mut Vec<String>,
) -> Result<(), String> {
    smoke_require(
        this,
        &format!("{name} action state is {active}"),
        |window| {
            window
                .actions
                .get(name)
                .and_then(|action| action.state())
                .and_then(|state| state.get::<bool>())
                == Some(active)
        },
    )?;
    steps.push(format!("{name}={active}"));
    Ok(())
}

fn smoke_require_action_enabled(
    this: &Rc<RefCell<MainWindow>>,
    name: &str,
    enabled: bool,
) -> Result<(), String> {
    smoke_require(
        this,
        &format!("{name} action enabled is {enabled}"),
        |window| {
            window
                .actions
                .get(name)
                .is_some_and(|action| action.is_enabled() == enabled)
        },
    )
}

fn smoke_require_layout_contract(
    this: &Rc<RefCell<MainWindow>>,
    steps: &mut Vec<String>,
) -> Result<(), String> {
    smoke_require(
        this,
        "layout and initial visibility match Windows constants",
        |window| {
            let find_bar_widths = child_width_requests(&window.find_bar);
            let find_bar_heights = child_height_requests(&window.find_bar);
            let command_palette_heights = child_height_requests(&window.command_palette);
            let line_number_text = window
                .line_numbers_buffer
                .text(
                    &window.line_numbers_buffer.start_iter(),
                    &window.line_numbers_buffer.end_iter(),
                    true,
                )
                .to_string();
            let line_number_labels = line_number_text.lines().collect::<Vec<_>>();
            !window.show_find_bar
                && !window.show_search_results
                && !window.show_command_palette
                && window.show_line_numbers
                && !window.find_bar.is_visible()
                && !window.search_results_panel.is_visible()
                && !window.command_palette.is_visible()
                && window.line_numbers_scrolled.is_visible()
                && !line_number_labels.is_empty()
                && line_number_labels.first() == Some(&"1")
                && line_number_labels.len() <= 200
                && window.find_bar.height_request() == FIND_BAR_HEIGHT
                && find_bar_widths
                    == [
                        FIND_LABEL_WIDTH,
                        FIND_ENTRY_MIN_WIDTH,
                        REPLACE_LABEL_WIDTH,
                        FIND_ENTRY_MIN_WIDTH,
                        FIND_NAV_BUTTON_WIDTH,
                        FIND_NAV_BUTTON_WIDTH,
                        REPLACE_ONE_BUTTON_WIDTH,
                        REPLACE_ALL_BUTTON_WIDTH,
                        FIND_ALL_BUTTON_WIDTH,
                        FIND_CLOSE_BUTTON_WIDTH,
                    ]
                && find_bar_heights
                    .iter()
                    .all(|height| *height == FIND_CONTROL_HEIGHT)
                && window.command_palette.height_request() == COMMAND_PALETTE_HEIGHT
                && window.command_palette.spacing() == COMMAND_PALETTE_SPACING
                && window.command_palette.margin_start() == COMMAND_PALETTE_MARGIN
                && window.command_palette.margin_end() == COMMAND_PALETTE_MARGIN
                && window.command_palette.margin_top() == COMMAND_PALETTE_MARGIN
                && window.command_palette.margin_bottom() == COMMAND_PALETTE_MARGIN
                && command_palette_heights == [COMMAND_FILTER_HEIGHT, COMMAND_LIST_HEIGHT]
                && window.command_filter.height_request() == COMMAND_FILTER_HEIGHT
                && window.command_results_panel.height_request() == COMMAND_LIST_HEIGHT
                && window.command_results_panel.min_content_height() == COMMAND_LIST_HEIGHT
                && window.command_results_panel.max_content_height() == COMMAND_LIST_HEIGHT
                && window.search_results_panel.margin_start() == SEARCH_RESULTS_SIDE_MARGIN
                && window.search_results_panel.margin_end() == SEARCH_RESULTS_SIDE_MARGIN
                && window.search_results_panel.margin_top() == 0
                && window.search_results_panel.margin_bottom() == SEARCH_RESULTS_BOTTOM_MARGIN
                && window.search_results_panel.min_content_height() == SEARCH_RESULTS_LIST_HEIGHT
                && window.search_results_panel.max_content_height() == SEARCH_RESULTS_LIST_HEIGHT
                && window.line_numbers_scrolled.min_content_width() == LINE_NUMBER_WIDTH
                && window.line_numbers_scrolled.max_content_width() == LINE_NUMBER_WIDTH
                && window.status.line.width_request() == STATUS_LINE_WIDTH
                && window.status.chars.width_request() == STATUS_CHARS_WIDTH
                && window.status.selected.width_request() == STATUS_SELECTED_WIDTH
                && window.status.encoding.width_request() == STATUS_ENCODING_WIDTH
                && window.status.line_ending.width_request() == STATUS_LINE_ENDING_WIDTH
                && window.status.wrap.width_request() == STATUS_WRAP_WIDTH
                && window.status.save_state.width_request() == STATUS_SAVE_STATE_WIDTH
        },
    )?;
    steps.push("layout-status-contract".to_string());
    Ok(())
}

fn child_width_requests(container: &gtk::Box) -> Vec<i32> {
    let mut widths = Vec::new();
    let mut child = container.first_child();
    while let Some(widget) = child {
        widths.push(widget.width_request());
        child = widget.next_sibling();
    }
    widths
}

fn child_height_requests(container: &gtk::Box) -> Vec<i32> {
    let mut heights = Vec::new();
    let mut child = container.first_child();
    while let Some(widget) = child {
        heights.push(widget.height_request());
        child = widget.next_sibling();
    }
    heights
}

fn smoke_require_action_accels(
    this: &Rc<RefCell<MainWindow>>,
    action_name: &str,
    expected: &[&str],
    steps: &mut Vec<String>,
) -> Result<(), String> {
    let actual = smoke_read(this, |window| {
        window
            .application
            .accels_for_action(action_name)
            .into_iter()
            .map(|accel| accel.to_string())
            .collect::<Vec<_>>()
    })?;
    if actual
        .iter()
        .map(String::as_str)
        .eq(expected.iter().copied())
    {
        steps.push(format!("{action_name}:accels={}", expected.join(",")));
        Ok(())
    } else {
        Err(format!(
            "check failed: {action_name} accels were {:?}, expected {:?}",
            actual, expected
        ))
    }
}

fn smoke_require<F>(this: &Rc<RefCell<MainWindow>>, label: &str, check: F) -> Result<(), String>
where
    F: FnOnce(&MainWindow) -> bool,
{
    let window = this
        .try_borrow()
        .map_err(|_| format!("window state was busy while checking {label}"))?;
    if check(&window) {
        Ok(())
    } else {
        Err(format!("check failed: {label}"))
    }
}

fn smoke_env_path(name: &str) -> Result<PathBuf, String> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{name} was not set"))
}

fn smoke_read<T, F>(this: &Rc<RefCell<MainWindow>>, read: F) -> Result<T, String>
where
    F: FnOnce(&MainWindow) -> T,
{
    let window = this
        .try_borrow()
        .map_err(|_| "window state was busy while reading smoke state".to_string())?;
    Ok(read(&window))
}

fn smoke_set_recent_files(
    this: &Rc<RefCell<MainWindow>>,
    files: Vec<PathBuf>,
) -> Result<(), String> {
    let mut window = this
        .try_borrow_mut()
        .map_err(|_| "window state was busy while preparing smoke recent files".to_string())?;
    window.app.set_recent_files(files);
    window.rebuild_menu();
    window.update_menu_checks();
    Ok(())
}

fn smoke_wait_for_pending_save(this: &Rc<RefCell<MainWindow>>) -> Result<(), String> {
    for _ in 0..100 {
        {
            let mut window = this
                .try_borrow_mut()
                .map_err(|_| "window state was busy while polling smoke save".to_string())?;
            window
                .poll_pending_save()
                .map_err(|error| error.user_message())?;
            if window.pending_save.is_none() {
                return Ok(());
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Err("background save did not finish during action smoke".to_string())
}

fn smoke_flush_persistence(this: &Rc<RefCell<MainWindow>>) -> Result<(), String> {
    let mut window = this
        .try_borrow_mut()
        .map_err(|_| "window state was busy while flushing smoke persistence".to_string())?;
    window.flush_pending_persistence_report_errors();
    if let Some(error) = window.last_persist_error.as_ref() {
        return Err(format!("persistence flush failed: {error}"));
    }
    if window.pending_persistence.has_pending() {
        return Err("persistence flush left pending changes".to_string());
    }
    Ok(())
}

fn smoke_insert_text(this: &Rc<RefCell<MainWindow>>, text: &str) -> Result<(), String> {
    let mut window = this
        .try_borrow_mut()
        .map_err(|_| "window state was busy while preparing smoke edit".to_string())?;
    let mut end = window.buffer.end_iter();
    window.buffer.insert(&mut end, text);
    window.edit_content_pending_sync = true;
    window
        .app
        .mark_current_dirty_from_view()
        .map_err(|error| error.user_message())?;
    window
        .sync_current_text()
        .map_err(|error| error.user_message())?;
    window.update_status();
    window.update_menu_checks();
    Ok(())
}

fn smoke_set_mixed_clipboard(
    this: &Rc<RefCell<MainWindow>>,
    plain: &str,
    html: &str,
) -> Result<(), String> {
    let window = this
        .try_borrow()
        .map_err(|_| "window state was busy while preparing smoke clipboard".to_string())?;
    let clipboard = gtk::prelude::WidgetExt::display(&window.window).clipboard();
    let html_bytes = glib::Bytes::from_owned(html.as_bytes().to_vec());
    let html_provider = gdk::ContentProvider::for_bytes("text/html", &html_bytes);
    let plain_value = plain.to_value();
    let plain_provider = gdk::ContentProvider::for_value(&plain_value);
    let provider = gdk::ContentProvider::new_union(&[html_provider, plain_provider]);
    clipboard
        .set_content(Some(&provider))
        .map_err(|error| format!("mixed clipboard content could not be set: {error}"))
}

fn smoke_set_find_replace(
    this: &Rc<RefCell<MainWindow>>,
    query: &str,
    replacement: &str,
) -> Result<(), String> {
    let mut window = this
        .try_borrow_mut()
        .map_err(|_| "window state was busy while preparing smoke replace".to_string())?;
    window.find_entry.set_text(query);
    window.replace_entry.set_text(replacement);
    window.on_find_query_changed();
    Ok(())
}

fn smoke_focus_find_entry_with_existing_query(
    this: &Rc<RefCell<MainWindow>>,
) -> Result<(), String> {
    let mut window = this
        .try_borrow_mut()
        .map_err(|_| "window state was busy while preparing smoke find focus".to_string())?;
    let query = window
        .app
        .current_document()
        .and_then(|document| document.content().split_whitespace().next())
        .ok_or_else(|| "current document had no text for smoke find focus".to_string())?
        .to_string();
    window.show_find_bar = true;
    window.show_or_hide_find_bar();
    window.find_entry.set_text(&query);
    window.on_find_query_changed();
    let start = window.buffer.start_iter();
    window.buffer.place_cursor(&start);
    window.focus_find_entry();
    Ok(())
}

fn smoke_run_find_entry_search(
    this: &Rc<RefCell<MainWindow>>,
    steps: &mut Vec<String>,
) -> Result<(), String> {
    let mut window = this
        .try_borrow_mut()
        .map_err(|_| "window state was busy while running smoke find focus".to_string())?;
    window
        .search_from_find_entry(SearchDirection::Forward)
        .map_err(|error| error.user_message())?;
    if window.find_entry_has_focus() && window.buffer.selection_bounds().is_some() {
        steps.push("find-entry-enter-preserves-focus".to_string());
        Ok(())
    } else {
        let focus = gtk::prelude::GtkWindowExt::focus(&window.window)
            .map(|widget| widget.type_().name().to_string())
            .unwrap_or_else(|| "none".to_string());
        Err(format!(
            "check failed: find entry Enter did not preserve focus and selection \
             (find_focus={}, selection={}, focus_widget={}, query={:?}, text_len={})",
            window.find_entry_has_focus(),
            window.buffer.selection_bounds().is_some(),
            focus,
            window.find_entry.text(),
            window.buffer_text().len()
        ))
    }
}

fn smoke_set_dirty_prompt_decision(
    this: &Rc<RefCell<MainWindow>>,
    decision: DialogChoice,
) -> Result<(), String> {
    let window = this
        .try_borrow()
        .map_err(|_| "window state was busy while preparing dirty prompt smoke".to_string())?;
    window.dirty_prompt_smoke_decision.set(Some(decision));
    Ok(())
}

fn smoke_confirm_all_dirty_before_exit(
    this: &Rc<RefCell<MainWindow>>,
    steps: &mut Vec<String>,
) -> Result<bool, String> {
    let mut window = this
        .try_borrow_mut()
        .map_err(|_| "window state was busy while confirming dirty exit smoke".to_string())?;
    let confirmed = window.confirm_all_dirty_before_exit();
    steps.push(format!("confirm-exit={confirmed}"));
    Ok(confirmed)
}

fn smoke_set_yes_no_prompt_decision(
    this: &Rc<RefCell<MainWindow>>,
    decision: DialogChoice,
) -> Result<(), String> {
    let window = this
        .try_borrow()
        .map_err(|_| "window state was busy while preparing yes/no prompt smoke".to_string())?;
    window.yes_no_prompt_smoke_decision.set(Some(decision));
    Ok(())
}

fn smoke_activate_command_palette_filter(
    this: &Rc<RefCell<MainWindow>>,
    filter: &str,
) -> Result<(), String> {
    let mut window = this
        .try_borrow_mut()
        .map_err(|_| "window state was busy while preparing command palette smoke".to_string())?;
    window.command_filter.set_text(filter);
    window
        .update_command_palette_filter()
        .map_err(|error| error.user_message())?;
    window
        .activate_selected_command()
        .map_err(|error| error.user_message())
}

fn launch_document_in_new_window(path: &Path) -> Result<(), AppError> {
    if env::var_os(ACTION_SMOKE_ENV).is_some() {
        if let Some(report_path) = env::var_os(ACTION_SMOKE_NEW_WINDOW_PATH_ENV) {
            fs::write(PathBuf::from(report_path), path.display().to_string())
                .map_err(|source| AppError::io(source, "write new-window smoke path"))?;
        }
        return Ok(());
    }

    let executable =
        env::current_exe().map_err(|source| AppError::io(source, "find app executable"))?;
    Command::new(&executable)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|source| {
            AppError::io_path_with_user(
                source,
                "launch new window",
                executable,
                "open in new window",
                path.to_path_buf(),
            )
        })?;
    Ok(())
}

fn build_menu_model(app: &EditorApp) -> gio::Menu {
    let root = gio::Menu::new();
    let shortcuts = &app.settings().shortcuts;

    let file = gio::Menu::new();
    let file_open = gio::Menu::new();
    file_open.append(
        Some(&command_menu_label(
            "New",
            shortcuts.shortcut_for(EditorCommandId::NewFile),
        )),
        Some("win.new"),
    );
    file_open.append(
        Some(&command_menu_label(
            "Open...",
            shortcuts.shortcut_for(EditorCommandId::OpenFile),
        )),
        Some("win.open"),
    );
    let recent = gio::Menu::new();
    if app.recent_files().is_empty() {
        recent.append(Some("(None)"), None::<&str>);
    } else {
        for (index, path) in app
            .recent_files()
            .iter()
            .take(crate::domain::MAX_RECENT_FILES)
            .enumerate()
        {
            let action = format!("win.recent-{index}");
            let label = format!("{} {}", index + 1, path.display());
            recent.append(Some(&label), Some(&action));
        }
    }
    file_open.append_submenu(Some("Recent"), &recent);
    file.append_section(None::<&str>, &file_open);
    let file_save = gio::Menu::new();
    file_save.append(
        Some(&command_menu_label(
            "Save",
            shortcuts.shortcut_for(EditorCommandId::Save),
        )),
        Some("win.save"),
    );
    file_save.append(
        Some(&command_menu_label(
            "Save As...",
            shortcuts.shortcut_for(EditorCommandId::SaveAs),
        )),
        Some("win.save-as"),
    );
    file.append_section(None::<&str>, &file_save);
    let file_close = gio::Menu::new();
    file_close.append(
        Some(&command_menu_label(
            "Close",
            shortcuts.shortcut_for(EditorCommandId::CloseTab),
        )),
        Some("win.close-tab"),
    );
    file_close.append(Some("Close All"), Some("win.close-all-tabs"));
    file.append_section(None::<&str>, &file_close);
    let file_exit = gio::Menu::new();
    file_exit.append(Some("Exit"), Some("win.exit"));
    file.append_section(None::<&str>, &file_exit);
    root.append_submenu(Some("File"), &file);

    let edit = gio::Menu::new();
    let edit_undo = gio::Menu::new();
    edit_undo.append(
        Some(&command_menu_label(
            "Undo",
            shortcuts.shortcut_for(EditorCommandId::Undo),
        )),
        Some("win.undo"),
    );
    edit_undo.append(
        Some(&command_menu_label(
            "Redo",
            shortcuts.shortcut_for(EditorCommandId::Redo),
        )),
        Some("win.redo"),
    );
    edit.append_section(None::<&str>, &edit_undo);
    let edit_clipboard = gio::Menu::new();
    edit_clipboard.append(
        Some(&command_menu_label(
            "Cut",
            shortcuts.shortcut_for(EditorCommandId::Cut),
        )),
        Some("win.cut"),
    );
    edit_clipboard.append(
        Some(&command_menu_label(
            "Copy",
            shortcuts.shortcut_for(EditorCommandId::Copy),
        )),
        Some("win.copy"),
    );
    edit_clipboard.append(
        Some(&command_menu_label(
            "Paste",
            shortcuts.shortcut_for(EditorCommandId::Paste),
        )),
        Some("win.paste"),
    );
    edit_clipboard.append(
        Some(&command_menu_label(
            "Select All",
            shortcuts.shortcut_for(EditorCommandId::SelectAll),
        )),
        Some("win.select-all"),
    );
    edit.append_section(None::<&str>, &edit_clipboard);
    root.append_submenu(Some("Edit"), &edit);

    let find = gio::Menu::new();
    let find_open = gio::Menu::new();
    find_open.append(
        Some(&command_menu_label(
            "Find...",
            shortcuts.shortcut_for(EditorCommandId::Find),
        )),
        Some("win.find"),
    );
    find_open.append(
        Some(&command_menu_label(
            "Replace...",
            shortcuts.shortcut_for(EditorCommandId::Replace),
        )),
        Some("win.replace"),
    );
    find.append_section(None::<&str>, &find_open);
    let find_search = gio::Menu::new();
    find_search.append(
        Some(&command_menu_label(
            "Find Next",
            shortcuts.shortcut_for(EditorCommandId::FindNext),
        )),
        Some("win.find-next"),
    );
    find_search.append(
        Some(&command_menu_label(
            "Find Prev",
            shortcuts.shortcut_for(EditorCommandId::FindPrevious),
        )),
        Some("win.find-previous"),
    );
    find_search.append(
        Some(&command_menu_label(
            "Find All",
            shortcuts.shortcut_for(EditorCommandId::FindAll),
        )),
        Some("win.find-all"),
    );
    find.append_section(None::<&str>, &find_search);
    root.append_submenu(Some("Find"), &find);

    let view = gio::Menu::new();
    let command_palette = gio::Menu::new();
    command_palette.append(
        Some(&command_menu_label(
            "Commands...",
            shortcuts.shortcut_for(EditorCommandId::CommandPalette),
        )),
        Some("win.command-palette"),
    );
    view.append_section(None::<&str>, &command_palette);
    let view_toggles = gio::Menu::new();
    view_toggles.append(Some("Line Numbers"), Some("win.line-numbers"));
    view_toggles.append(Some("Marks"), Some("win.visible-whitespace"));
    view_toggles.append(
        Some(&command_menu_label(
            "Word Wrap",
            shortcuts.shortcut_for(EditorCommandId::ToggleWordWrap),
        )),
        Some("win.word-wrap"),
    );
    view.append_section(None::<&str>, &view_toggles);
    let themes = gio::Menu::new();
    for theme in ThemeMode::options() {
        themes.append(
            Some(theme.display_name()),
            Some(&format!("win.{}", theme_action_name(*theme))),
        );
    }
    let theme_section = gio::Menu::new();
    theme_section.append_submenu(Some("Theme"), &themes);
    view.append_section(None::<&str>, &theme_section);
    root.append_submenu(Some("View"), &view);

    let tabs = gio::Menu::new();
    let tab_move = gio::Menu::new();
    tab_move.append(Some("Move Left"), Some("win.tab-left"));
    tab_move.append(Some("Move Right"), Some("win.tab-right"));
    tabs.append_section(None::<&str>, &tab_move);
    let tab_close = gio::Menu::new();
    tab_close.append(
        Some(&command_menu_label(
            "Close",
            shortcuts.shortcut_for(EditorCommandId::CloseTab),
        )),
        Some("win.close-tab"),
    );
    tab_close.append(Some("Close Others"), Some("win.close-other-tabs"));
    tab_close.append(Some("Close All"), Some("win.close-all-tabs"));
    tabs.append_section(None::<&str>, &tab_close);
    root.append_submenu(Some("Tabs"), &tabs);

    let text = gio::Menu::new();
    let encoding = gio::Menu::new();
    encoding.append(Some("Reopen Encoding..."), Some("win.reopen-encoding"));
    encoding.append(Some("Change Encoding..."), Some("win.change-encoding"));
    text.append_section(None::<&str>, &encoding);
    let line_endings = gio::Menu::new();
    line_endings.append(Some("CRLF"), Some("win.line-ending-crlf"));
    line_endings.append(Some("LF"), Some("win.line-ending-lf"));
    line_endings.append(Some("CR"), Some("win.line-ending-cr"));
    let line_ending_section = gio::Menu::new();
    line_ending_section.append_submenu(Some("Line Ends"), &line_endings);
    text.append_section(None::<&str>, &line_ending_section);
    root.append_submenu(Some("Text"), &text);

    let settings = gio::Menu::new();
    let settings_main = gio::Menu::new();
    settings_main.append(Some("Font..."), Some("win.choose-font"));
    let tab_size = gio::Menu::new();
    tab_size.append(Some("2 spaces"), Some("win.tab-size-2"));
    tab_size.append(Some("4 spaces"), Some("win.tab-size-4"));
    tab_size.append(Some("8 spaces"), Some("win.tab-size-8"));
    settings_main.append_submenu(Some("Tab Size"), &tab_size);
    let shortcuts = gio::Menu::new();
    for (index, command) in EditorCommandId::SHORTCUT_COMMANDS
        .iter()
        .copied()
        .enumerate()
    {
        let current = app.settings().shortcuts.shortcut_for(command);
        let default = command.default_shortcut();
        let title = command.shortcut_title().unwrap_or("Command");
        let command_menu = gio::Menu::new();
        let current_section = gio::Menu::new();
        current_section.append(Some(&shortcut_current_label(current)), None::<&str>);
        command_menu.append_section(None::<&str>, &current_section);
        let actions = gio::Menu::new();
        actions.append(
            Some("Set..."),
            Some(&format!("win.shortcut-{index}-capture")),
        );
        actions.append(
            Some(&format!(
                "Default: {}",
                optional_shortcut_display_name(default)
            )),
            Some(&format!("win.shortcut-{index}-default")),
        );
        actions.append(Some("Off"), Some(&format!("win.shortcut-{index}-disable")));
        command_menu.append_section(None::<&str>, &actions);
        shortcuts.append_submenu(Some(&shortcut_menu_title(title, current)), &command_menu);
    }
    settings_main.append_submenu(Some("Shortcuts"), &shortcuts);
    settings.append_section(None::<&str>, &settings_main);
    root.append_submenu(Some("Settings"), &settings);

    let help = gio::Menu::new();
    help.append(Some("About"), Some("win.about"));
    root.append_submenu(Some("Help"), &help);

    root
}

fn build_tab_context_menu_model(app: &EditorApp) -> gio::Menu {
    let menu = gio::Menu::new();
    let open = gio::Menu::new();
    if app.document_count() > 1 {
        open.append(Some("Open in New Window"), Some("win.open-new-window"));
    } else {
        open.append(Some("Open in New Window"), None::<&str>);
    }
    menu.append_section(None::<&str>, &open);
    let close = gio::Menu::new();
    close.append(
        Some(&command_menu_label(
            "Close",
            app.settings()
                .shortcuts
                .shortcut_for(EditorCommandId::CloseTab),
        )),
        Some("win.close-tab"),
    );
    close.append(Some("Close Others"), Some("win.close-other-tabs"));
    menu.append_section(None::<&str>, &close);
    menu
}

fn build_editor_context_menu_model(app: &EditorApp) -> gio::Menu {
    let menu = gio::Menu::new();
    let shortcuts = &app.settings().shortcuts;
    let undo = gio::Menu::new();
    undo.append(
        Some(&command_menu_label(
            "Undo",
            shortcuts.shortcut_for(EditorCommandId::Undo),
        )),
        Some("win.undo"),
    );
    undo.append(
        Some(&command_menu_label(
            "Redo",
            shortcuts.shortcut_for(EditorCommandId::Redo),
        )),
        Some("win.redo"),
    );
    menu.append_section(None::<&str>, &undo);
    let clipboard = gio::Menu::new();
    clipboard.append(
        Some(&command_menu_label(
            "Cut",
            shortcuts.shortcut_for(EditorCommandId::Cut),
        )),
        Some("win.cut"),
    );
    clipboard.append(
        Some(&command_menu_label(
            "Copy",
            shortcuts.shortcut_for(EditorCommandId::Copy),
        )),
        Some("win.copy"),
    );
    clipboard.append(
        Some(&command_menu_label(
            "Paste",
            shortcuts.shortcut_for(EditorCommandId::Paste),
        )),
        Some("win.paste"),
    );
    clipboard.append(
        Some(&command_menu_label(
            "Select All",
            shortcuts.shortcut_for(EditorCommandId::SelectAll),
        )),
        Some("win.select-all"),
    );
    menu.append_section(None::<&str>, &clipboard);
    menu
}

fn status_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_xalign(0.0);
    label
}

fn focus_text_view(window: &gtk::ApplicationWindow, text_view: &gtk::TextView) {
    gtk::prelude::GtkWindowExt::set_focus(window, Some(text_view));
    text_view.grab_focus();
}

fn clear_list_box(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn set_buffer_text_without_undo(buffer: &gtk::TextBuffer, text: &str) {
    buffer.begin_irreversible_action();
    buffer.set_text(text);
    buffer.end_irreversible_action();
    buffer.set_modified(false);
}

struct ModalDialogGuard;

impl ModalDialogGuard {
    fn enter() -> Self {
        ACTIVE_MODAL_DIALOG_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }
}

impl Drop for ModalDialogGuard {
    fn drop(&mut self) {
        ACTIVE_MODAL_DIALOG_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

fn modal_dialog_active() -> bool {
    ACTIVE_MODAL_DIALOG_DEPTH.with(|depth| depth.get() > 0)
}

fn run_modal_future<F: Future>(future: F) -> F::Output {
    let _guard = ModalDialogGuard::enter();
    glib::MainContext::default().block_on(future)
}

#[allow(deprecated)]
fn run_choice_dialog(
    parent: Option<&gtk::ApplicationWindow>,
    title: &str,
    message: &str,
    message_type: gtk::MessageType,
    buttons: &[ChoiceButtonSpec],
) -> DialogChoice {
    let dialog = new_message_dialog(
        parent.map(|parent| parent.upcast_ref::<gtk::Window>()),
        title,
        message,
        message_type,
        gtk::ButtonsType::None,
    );
    for (label, response) in buttons {
        dialog.add_button(label, *response);
    }
    if let Some((_, response)) = buttons.first() {
        dialog.set_default_response(*response);
    }
    let response = run_modal_future(dialog.run_future());
    dialog.close();
    choice_from_dialog_response(response, buttons)
}

fn run_external_file_changed_dialog(
    parent: Option<&gtk::ApplicationWindow>,
    message: &str,
) -> ExternalFileChangedAction {
    let dialog = new_message_dialog(
        parent.map(|parent| parent.upcast_ref::<gtk::Window>()),
        FILE_CHANGED_DIALOG_TITLE,
        message,
        gtk::MessageType::Error,
        gtk::ButtonsType::None,
    );
    for (label, response) in EXTERNAL_FILE_CHANGED_BUTTONS {
        dialog.add_button(label, *response);
    }
    dialog.set_default_response(gtk::ResponseType::Cancel);
    let response = run_modal_future(dialog.run_future());
    dialog.close();
    external_file_changed_action_from_response(response)
}

fn external_file_changed_action_from_response(
    response: gtk::ResponseType,
) -> ExternalFileChangedAction {
    match response {
        gtk::ResponseType::Other(EXTERNAL_CHANGE_RELOAD_RESPONSE) => {
            ExternalFileChangedAction::Reload
        }
        gtk::ResponseType::Other(EXTERNAL_CHANGE_SAVE_AS_RESPONSE) => {
            ExternalFileChangedAction::SaveAs
        }
        _ => ExternalFileChangedAction::Cancel,
    }
}

fn choice_from_dialog_response(
    response: gtk::ResponseType,
    buttons: &[ChoiceButtonSpec],
) -> DialogChoice {
    match response {
        gtk::ResponseType::Yes | gtk::ResponseType::Ok | gtk::ResponseType::Accept => {
            DialogChoice::Yes
        }
        gtk::ResponseType::No => DialogChoice::No,
        _ if buttons
            .iter()
            .any(|(_, button_response)| *button_response == gtk::ResponseType::Cancel) =>
        {
            DialogChoice::Cancel
        }
        _ => DialogChoice::No,
    }
}

fn show_error_dialog(parent: Option<&gtk::ApplicationWindow>, error: &AppError) {
    show_message_dialog(
        parent,
        ERROR_DIALOG_TITLE,
        &error.user_message(),
        gtk::MessageType::Error,
    );
}

#[allow(deprecated)]
fn new_message_dialog(
    parent: Option<&gtk::Window>,
    title: &str,
    message: &str,
    message_type: gtk::MessageType,
    buttons: gtk::ButtonsType,
) -> gtk::MessageDialog {
    let content = message_dialog_content(title, message);
    let mut builder = gtk::MessageDialog::builder()
        .modal(true)
        .message_type(message_type)
        .buttons(buttons)
        .title(content.window_title)
        .text(content.body_text);
    if let Some(secondary_text) = content.secondary_text {
        builder = builder.secondary_text(secondary_text);
    }
    if let Some(parent) = parent {
        builder = builder.transient_for(parent);
    }
    builder.build()
}

fn message_dialog_content<'a>(title: &'a str, message: &'a str) -> MessageDialogContent<'a> {
    MessageDialogContent {
        window_title: title,
        body_text: message,
        secondary_text: None,
    }
}

#[allow(deprecated)]
fn show_message_dialog(
    parent: Option<&gtk::ApplicationWindow>,
    title: &str,
    message: &str,
    message_type: gtk::MessageType,
) {
    show_message_dialog_for_window(
        parent.map(|parent| parent.upcast_ref::<gtk::Window>()),
        title,
        message,
        message_type,
    );
}

#[allow(deprecated)]
fn show_message_dialog_for_window(
    parent: Option<&gtk::Window>,
    title: &str,
    message: &str,
    message_type: gtk::MessageType,
) {
    let dialog = new_message_dialog(parent, title, message, message_type, gtk::ButtonsType::Ok);
    let _ = run_modal_future(dialog.run_future());
    dialog.close();
}

#[allow(deprecated)]
fn show_about_dialog(parent: Option<&gtk::ApplicationWindow>) {
    let dialog = new_message_dialog(
        parent.map(|parent| parent.upcast_ref::<gtk::Window>()),
        ABOUT_DIALOG_TITLE,
        ABOUT_DIALOG_MESSAGE,
        gtk::MessageType::Info,
        gtk::ButtonsType::Ok,
    );
    let link = gtk::LinkButton::with_label(ABOUT_DIALOG_URL, ABOUT_DIALOG_URL);
    link.set_halign(gtk::Align::Start);
    if let Ok(message_area) = dialog.message_area().downcast::<gtk::Box>() {
        message_area.append(&link);
    } else {
        dialog.content_area().append(&link);
    }
    let _ = run_modal_future(dialog.run_future());
    dialog.close();
}

fn save_target_expectation(
    document_path: Option<&Path>,
    save_path: &Path,
    document_snapshot: Option<FileSnapshot>,
    backing_file_missing: bool,
    force_save_as: bool,
    selected_target_expectation: Option<SaveTargetExpectation>,
) -> Result<SaveTargetExpectation, AppError> {
    if let Some(expectation) = selected_target_expectation {
        return Ok(expectation);
    }
    if document_path == Some(save_path) {
        Ok(if let Some(snapshot) = document_snapshot {
            SaveTargetExpectation::Unchanged(snapshot)
        } else if backing_file_missing && !force_save_as {
            SaveTargetExpectation::Missing
        } else {
            SaveTargetExpectation::Any
        })
    } else {
        Err(AppError::InvalidState(
            "Save target expectation was not captured for selected path",
        ))
    }
}

fn selected_save_target_expectation(
    io: &FileDocumentIo,
    save_path: &Path,
) -> Result<SaveTargetExpectation, AppError> {
    match io.file_metadata_snapshot(save_path) {
        Ok(metadata) => {
            if metadata.byte_len() > MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES {
                return Ok(SaveTargetExpectation::UnchangedMetadata(metadata));
            }
            let snapshot = io.file_snapshot(save_path)?;
            Ok(SaveTargetExpectation::UnchangedWithMetadata { snapshot, metadata })
        }
        Err(error) if error.file_access_kind() == Some(FileAccessKind::NotFound) => {
            Ok(SaveTargetExpectation::Missing)
        }
        Err(error) => Err(error),
    }
}

fn save_target_needs_overwrite_confirmation(expectation: SaveTargetExpectation) -> bool {
    !matches!(expectation, SaveTargetExpectation::Missing)
}

fn save_as_overwrite_message(path: &Path) -> String {
    let target = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map_or_else(|| path.display().to_string(), ToOwned::to_owned);
    format!("{target} already exists.\n\nDo you want to replace it?")
}

fn visible_whitespace_display_text<'a>(
    cache: &mut HashMap<crate::domain::DocumentId, VisibleWhitespaceDisplayCache>,
    key: VisibleWhitespaceDisplayCacheKey,
    source: &'a str,
) -> VisibleWhitespaceDisplayText<'a> {
    if !key.show_whitespace {
        cache.clear();
        return VisibleWhitespaceDisplayText::Source(source);
    }
    if !can_render_visible_whitespace_bytes(source.len()) {
        cache.remove(&key.document_id);
        return VisibleWhitespaceDisplayText::Source(source);
    }
    if let Some(cached) = cache
        .get(&key.document_id)
        .filter(|cached| cached.matches(key))
    {
        return cached.display_text(source);
    }
    let rendered = match render_visible_whitespace_for_display(source) {
        Some(Cow::Borrowed(_)) => None,
        Some(Cow::Owned(rendered)) => Some(rendered),
        None => {
            cache.remove(&key.document_id);
            return VisibleWhitespaceDisplayText::Source(source);
        }
    };
    cache.insert(
        key.document_id,
        VisibleWhitespaceDisplayCache::new(key, rendered),
    );
    cache
        .get(&key.document_id)
        .map_or(VisibleWhitespaceDisplayText::Source(source), |cached| {
            cached.display_text(source)
        })
}

fn validate_selection_replacement_document_size(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    replacement: &str,
) -> Result<(), AppError> {
    let new_len = text
        .len()
        .checked_sub(end_byte.saturating_sub(start_byte))
        .and_then(|len| len.checked_add(replacement.len()))
        .ok_or(AppError::FileTooLarge {
            path: PathBuf::new(),
            byte_len: u64::MAX,
            limit: MAX_DOCUMENT_LOAD_BYTES,
        })?;
    validate_editor_document_byte_len(new_len)
}

fn replace_all_text_if_changed(
    text: &str,
    query: &str,
    replacement: &str,
) -> Result<Option<ReplaceAllText>, AppError> {
    if query.is_empty() || !text.contains(query) {
        return Ok(None);
    }
    let occurrences = text.match_indices(query).count();
    let removed = occurrences
        .checked_mul(query.len())
        .ok_or_else(replacement_size_error)?;
    let added = occurrences
        .checked_mul(replacement.len())
        .ok_or_else(replacement_size_error)?;
    let new_len = text
        .len()
        .checked_sub(removed)
        .and_then(|len| len.checked_add(added))
        .ok_or_else(replacement_size_error)?;
    validate_editor_document_byte_len(new_len)?;
    let replaced = text.replace(query, replacement);
    let scan = LoadedTextAnalysis::scan_text(&replaced);
    if scan.contains_nul {
        return Err(AppError::encoding_unsafe_text(
            "Replacement produced NUL text",
        ));
    }
    Ok(Some(ReplaceAllText {
        text: replaced,
        metrics: scan.analysis.metrics,
    }))
}

fn validate_editor_document_byte_len(byte_len: usize) -> Result<(), AppError> {
    let byte_len_u64 = u64::try_from(byte_len).unwrap_or(u64::MAX);
    if !can_load_document_bytes(byte_len_u64) {
        return Err(AppError::file_too_large(
            PathBuf::new(),
            byte_len_u64,
            MAX_DOCUMENT_LOAD_BYTES,
        ));
    }
    Ok(())
}

fn replacement_size_error() -> AppError {
    AppError::file_too_large(PathBuf::new(), u64::MAX, MAX_DOCUMENT_LOAD_BYTES)
}

fn disable_visible_whitespace_for_oversized_current_document(app: &mut EditorApp) -> bool {
    if app.settings().show_whitespace
        && app
            .current_document()
            .is_some_and(|document| !can_render_visible_whitespace_bytes(document.content().len()))
    {
        let mut settings = app.settings().clone();
        settings.show_whitespace = false;
        app.set_settings(settings);
        return true;
    }
    false
}

fn visible_whitespace_size_limit_message(byte_len: usize) -> String {
    let limit_mb = VISIBLE_WHITESPACE_RENDER_LIMIT_BYTES as f64 / (1024.0 * 1024.0);
    let document_mb = byte_len as f64 / (1024.0 * 1024.0);
    format!(
        "Marks work up to {limit_mb:.0} MB.\n\nThis file is {document_mb:.1} MB, so normal view stays on."
    )
}

fn update_encoding_status_label(entry: &gtk::Entry, status_label: &gtk::Label) {
    let (_, message) = encoding_dialog_validation(entry.text().as_str());
    status_label.set_text(&message);
}

fn encoding_from_dialog_input(input: &str) -> Option<TextEncoding> {
    TextEncoding::from_user_input(input.trim())
}

fn encoding_dialog_validation(input: &str) -> (Option<TextEncoding>, String) {
    let trimmed = input.trim();
    let encoding = TextEncoding::from_user_input(trimmed);
    let message = match encoding {
        Some(encoding) => format!("OK: {}", encoding.display_name()),
        None if trimmed.is_empty() => "Pick an encoding.".to_string(),
        None => "This encoding is not supported.".to_string(),
    };
    (encoding, message)
}

fn invalid_encoding_message(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        "Pick an encoding.".to_string()
    } else {
        format!("\"{trimmed}\" is not supported.\n\nTry UTF-8, CP949, or Windows-1252.")
    }
}

fn editor_save_state_label(status: &CurrentEditorStatus) -> &'static str {
    if status.can_save {
        "Can Save"
    } else if status.can_save_as {
        "Save As"
    } else {
        "No Save"
    }
}

fn editor_window_title(status: &CurrentEditorStatus) -> String {
    let title = if status.title.is_empty() {
        APP_TITLE
    } else {
        status.title.as_str()
    };
    format!("{title} - {APP_TITLE}")
}

fn editor_status_state_text(status: &CurrentEditorStatus) -> String {
    let target = if let Some(path) = &status.path {
        path.display().to_string()
    } else if status.title.is_empty() {
        APP_TITLE.to_string()
    } else {
        status.title.clone()
    };
    format!("{} | {target}", status.status_kind.label())
}

fn command_label(command: EditorCommand) -> String {
    format!("{}: {}", command.group.display_name(), command.title)
}

fn command_menu_label(title: &str, shortcut: Option<KeyboardShortcut>) -> String {
    shortcut
        .map(|shortcut| format!("{title}\t{}", shortcut.display_name()))
        .unwrap_or_else(|| title.to_string())
}

fn optional_shortcut_display_name(shortcut: Option<KeyboardShortcut>) -> String {
    shortcut
        .map(KeyboardShortcut::display_name)
        .unwrap_or_else(|| "Off".to_string())
}

fn shortcut_current_label(shortcut: Option<KeyboardShortcut>) -> String {
    format!("Now: {}", optional_shortcut_display_name(shortcut))
}

fn shortcut_menu_title(title: &str, shortcut: Option<KeyboardShortcut>) -> String {
    format!("{title} ({})", optional_shortcut_display_name(shortcut))
}

fn shortcut_to_gtk_accel(shortcut: KeyboardShortcut) -> String {
    let mut accel = String::new();
    if shortcut.ctrl {
        accel.push_str("<Control>");
    }
    if shortcut.alt {
        accel.push_str("<Alt>");
    }
    if shortcut.shift {
        accel.push_str("<Shift>");
    }
    match shortcut.key {
        ShortcutKey::Character(ch) => accel.push(ch.to_ascii_uppercase()),
        ShortcutKey::Function(number) => accel.push_str(&format!("F{number}")),
    }
    accel
}

fn action_name_for_command(command: EditorCommandId) -> &'static str {
    match command {
        EditorCommandId::NewFile => "win.new",
        EditorCommandId::OpenFile => "win.open",
        EditorCommandId::Save => "win.save",
        EditorCommandId::SaveAs => "win.save-as",
        EditorCommandId::CloseTab => "win.close-tab",
        EditorCommandId::Find => "win.find",
        EditorCommandId::Replace => "win.replace",
        EditorCommandId::FindAll => "win.find-all",
        EditorCommandId::FindNext => "win.find-next",
        EditorCommandId::FindPrevious => "win.find-previous",
        EditorCommandId::CommandPalette => "win.command-palette",
        EditorCommandId::ToggleWordWrap => "win.word-wrap",
        EditorCommandId::SelectAll => "win.select-all",
        EditorCommandId::Undo => "win.undo",
        EditorCommandId::Redo => "win.redo",
        EditorCommandId::Cut => "win.cut",
        EditorCommandId::Copy => "win.copy",
        EditorCommandId::Paste => "win.paste",
        EditorCommandId::CloseOtherTabs => "win.close-other-tabs",
        EditorCommandId::ToggleLineNumbers => "win.line-numbers",
        EditorCommandId::ToggleVisibleWhitespace => "win.visible-whitespace",
        EditorCommandId::ReopenWithEncoding => "win.reopen-encoding",
        EditorCommandId::ConvertEncoding => "win.change-encoding",
        EditorCommandId::SetLineEnding(LineEnding::Crlf) => "win.line-ending-crlf",
        EditorCommandId::SetLineEnding(LineEnding::Lf) => "win.line-ending-lf",
        EditorCommandId::SetLineEnding(LineEnding::Cr) => "win.line-ending-cr",
    }
}

fn editor_control_shortcut_command(command: EditorCommandId) -> bool {
    matches!(
        command,
        EditorCommandId::Undo
            | EditorCommandId::Redo
            | EditorCommandId::Cut
            | EditorCommandId::Copy
            | EditorCommandId::Paste
            | EditorCommandId::SelectAll
    )
}

fn shortcut_from_key_event(
    key: gdk::Key,
    modifiers: gdk::ModifierType,
) -> Option<KeyboardShortcut> {
    let shortcut = KeyboardShortcut {
        ctrl: modifiers.contains(gdk::ModifierType::CONTROL_MASK),
        alt: modifiers.contains(gdk::ModifierType::ALT_MASK),
        shift: modifiers.contains(gdk::ModifierType::SHIFT_MASK),
        key: shortcut_key_from_gdk_key(key)?,
    };
    shortcut.is_safe_for_text_editor().then_some(shortcut)
}

fn shortcut_key_from_gdk_key(key: gdk::Key) -> Option<ShortcutKey> {
    if let Some(name) = key.name() {
        let name = name.as_str();
        if name.starts_with("KP_") {
            return None;
        }
        if let Some(number) = name.strip_prefix('F')
            && let Ok(number) = number.parse::<u8>()
            && (1..=24).contains(&number)
        {
            return Some(ShortcutKey::Function(number));
        }
    }

    key.to_unicode()
        .filter(char::is_ascii_alphanumeric)
        .map(|ch| ShortcutKey::Character(ch.to_ascii_uppercase()))
}

fn theme_action_name(theme: ThemeMode) -> &'static str {
    match theme {
        ThemeMode::System => "theme-system",
        ThemeMode::Light => "theme-light",
        ThemeMode::ClassicDark => "theme-classic-dark",
        ThemeMode::SepiaTeal => "theme-sepia-teal",
        ThemeMode::Graphite => "theme-graphite",
        ThemeMode::Forest => "theme-forest",
        ThemeMode::SteelBlue => "theme-steel-blue",
    }
}

#[allow(deprecated)]
fn configure_file_dialog_filters(dialog: &gtk::FileChooserNative) {
    let filters = file_dialog_filters();
    for filter in filters {
        dialog.add_filter(&filter);
        if dialog.filter().is_none() {
            dialog.set_filter(&filter);
        }
    }
}

fn file_dialog_filters() -> Vec<gtk::FileFilter> {
    FILE_DIALOG_FILTERS
        .iter()
        .map(|spec| {
            let filter = gtk::FileFilter::new();
            filter.set_name(Some(spec.name));
            filter.add_pattern(spec.pattern);
            filter
        })
        .collect()
}

#[cfg(test)]
fn file_dialog_filter_specs() -> &'static [FileDialogFilterSpec] {
    FILE_DIALOG_FILTERS
}

fn editor_font_description(settings: &EditorSettings) -> pango::FontDescription {
    let mut description = pango::FontDescription::new();
    description.set_family(&settings.font_name);
    description.set_size(pango_font_size_units(settings.font_size_pt));
    description
}

fn pango_font_size_units(font_size_pt: u32) -> i32 {
    font_size_pt
        .min((i32::MAX / pango::SCALE) as u32)
        .saturating_mul(pango::SCALE as u32) as i32
}

fn font_size_points_from_pango_size(pango_size: i32, fallback_size_pt: u32) -> u32 {
    if pango_size <= 0 {
        return fallback_size_pt.clamp(MIN_FONT_SIZE_PT, MAX_FONT_SIZE_PT);
    }
    let rounded = ((i64::from(pango_size) + i64::from(pango::SCALE / 2)) / i64::from(pango::SCALE))
        .clamp(i64::from(MIN_FONT_SIZE_PT), i64::from(MAX_FONT_SIZE_PT));
    rounded as u32
}

fn tab_width_pixels(tab_size: u8, approximate_char_width: i32) -> i32 {
    let tab_size = i64::from(tab_size.max(1));
    let char_width = i64::from(approximate_char_width.max(pango::SCALE));
    (((char_width * tab_size) + i64::from(pango::SCALE / 2)) / i64::from(pango::SCALE))
        .min(i64::from(i32::MAX)) as i32
}

fn resolve_theme_mode(theme: ThemeMode, system_prefers_dark: bool) -> ThemeMode {
    match theme {
        ThemeMode::System if system_prefers_dark => ThemeMode::ClassicDark,
        ThemeMode::System => ThemeMode::Light,
        theme => theme,
    }
}

fn system_prefers_dark_theme() -> bool {
    gtk::Settings::default()
        .map(|settings| gtk_settings_prefers_dark_theme(&settings))
        .unwrap_or(false)
}

fn gtk_settings_prefers_dark_theme(settings: &gtk::Settings) -> bool {
    settings.is_gtk_application_prefer_dark_theme()
        || settings
            .gtk_theme_name()
            .map(|theme_name| gtk_theme_name_prefers_dark(theme_name.as_str()))
            .unwrap_or(false)
}

fn gtk_theme_name_prefers_dark(theme_name: &str) -> bool {
    theme_name
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case("dark"))
}

fn char_offset_to_byte_index(text: &str, char_offset: usize) -> usize {
    if char_offset == 0 {
        return 0;
    }
    text.char_indices()
        .nth(char_offset)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn byte_index_to_char_offset(text: &str, byte_index: usize) -> i32 {
    let index = floor_char_boundary(text, byte_index.min(text.len()));
    text[..index].chars().count().min(i32::MAX as usize) as i32
}

fn char_offset_to_utf16_offset(text: &str, char_offset: usize) -> usize {
    let byte_index = char_offset_to_byte_index(text, char_offset);
    byte_index_to_utf16_offset(text, byte_index)
}

fn line_and_utf16_column_from_char_offset(text: &str, char_offset: usize) -> (u32, u32) {
    let target_byte = char_offset_to_byte_index(text, char_offset);
    let mut line = 1usize;
    let mut line_start_utf16 = 0usize;
    let mut utf16_offset = 0usize;
    let mut previous_was_cr = false;

    for ch in text[..target_byte].chars() {
        utf16_offset = utf16_offset.saturating_add(ch.len_utf16());
        match ch {
            '\r' => {
                line = line.saturating_add(1);
                line_start_utf16 = utf16_offset;
                previous_was_cr = true;
            }
            '\n' if previous_was_cr => {
                line_start_utf16 = utf16_offset;
                previous_was_cr = false;
            }
            '\n' => {
                line = line.saturating_add(1);
                line_start_utf16 = utf16_offset;
                previous_was_cr = false;
            }
            _ => {
                previous_was_cr = false;
            }
        }
    }

    let column = utf16_offset
        .saturating_sub(line_start_utf16)
        .saturating_add(1);
    (
        line.min(u32::MAX as usize) as u32,
        column.min(u32::MAX as usize) as u32,
    )
}

fn point_in_rect(x: f64, y: f64, rect_x: f64, rect_y: f64, width: f64, height: f64) -> bool {
    if width <= 0.0 || height <= 0.0 {
        return false;
    }
    x >= rect_x && x < rect_x + width && y >= rect_y && y < rect_y + height
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn new(prefix: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos();
            let path = env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
            fs::create_dir(&path).expect("create temp test directory");
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

    fn run_gtk_test_or_skip<F>(test_name: &str, test: F)
    where
        F: FnOnce() + Send + std::panic::UnwindSafe + 'static,
    {
        let started = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started_in_test = std::sync::Arc::clone(&started);
        let result = std::panic::catch_unwind(|| {
            gtk::test_synced(move || {
                started_in_test.store(true, std::sync::atomic::Ordering::Release);
                test();
            });
        });
        if let Err(panic) = result {
            if started.load(std::sync::atomic::Ordering::Acquire) {
                std::panic::resume_unwind(panic);
            }
            eprintln!("skipping {test_name} because GTK test initialization failed");
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    struct MenuItemSnapshot {
        label: String,
        action: Option<String>,
        submenu: bool,
    }

    #[test]
    fn long_tab_tooltip_paths_wrap_at_path_separators() {
        let path = "C:\\very\\long\\nested\\directory\\structure\\file.txt";
        let wrapped = MainWindow::wrap_long_tab_tooltip_path(path, 18);

        assert_eq!(
            wrapped,
            "C:\\very\\long\\\r\nnested\\directory\\\r\nstructure\\file.txt"
        );
        assert_eq!(wrapped.replace("\r\n", ""), path);
        assert!(wrapped.split("\r\n").all(|line| line.chars().count() <= 18));
    }

    #[test]
    fn long_tab_tooltip_paths_wrap_long_segments_without_separators() {
        let wrapped = MainWindow::wrap_long_tab_tooltip_path("abcdefghij", 4);

        assert_eq!(wrapped, "abcd\r\nefgh\r\nij");
    }

    #[test]
    fn short_tab_tooltip_paths_remain_single_line() {
        let path = "C:\\short.txt";

        assert_eq!(MainWindow::wrap_long_tab_tooltip_path(path, 80), path);
    }

    #[test]
    fn startup_file_paths_skip_executable_and_empty_args_like_windows() {
        let paths = startup_file_paths_from_args([
            OsString::from("j3text"),
            OsString::from(""),
            OsString::from("note.txt"),
            OsString::from("/tmp/second.txt"),
        ]);

        assert_eq!(
            paths,
            vec![PathBuf::from("note.txt"), PathBuf::from("/tmp/second.txt")]
        );
    }

    #[test]
    fn external_file_changed_responses_map_to_actions() {
        assert_eq!(
            external_file_changed_action_from_response(gtk::ResponseType::Other(
                EXTERNAL_CHANGE_RELOAD_RESPONSE
            )),
            ExternalFileChangedAction::Reload
        );
        assert_eq!(
            external_file_changed_action_from_response(gtk::ResponseType::Other(
                EXTERNAL_CHANGE_SAVE_AS_RESPONSE
            )),
            ExternalFileChangedAction::SaveAs
        );
        assert_eq!(
            external_file_changed_action_from_response(gtk::ResponseType::Cancel),
            ExternalFileChangedAction::Cancel
        );
        assert_eq!(
            external_file_changed_action_from_response(gtk::ResponseType::None),
            ExternalFileChangedAction::Cancel
        );
    }

    #[test]
    fn linux_change_marker_does_not_skip_fingerprint_conflict_guard() {
        let root = TempRoot::new("j3text-linux-change-marker-conflict-guard");
        let path = root.path().join("document.txt");
        fs::write(&path, b"unchanged").expect("write document");
        let io = FileDocumentIo::new();
        let metadata = io.file_metadata_snapshot(&path).expect("read metadata");

        if metadata.has_change_marker() {
            assert!(
                !metadata.is_confirmed_unchanged_from(metadata),
                "Linux ctime can share a timestamp across rapid same-size writes, so content fingerprint remains the conflict guard"
            );
        }
    }

    fn menu_item_string<M: IsA<gio::MenuModel>>(
        model: &M,
        index: i32,
        attribute: &str,
    ) -> Option<String> {
        model
            .item_attribute_value(index, attribute, Some(glib::VariantTy::STRING))
            .and_then(|value| value.get::<String>())
    }

    fn menu_item_label<M: IsA<gio::MenuModel>>(model: &M, index: i32) -> String {
        menu_item_string(model, index, "label").unwrap_or_default()
    }

    fn menu_item_action<M: IsA<gio::MenuModel>>(model: &M, index: i32) -> Option<String> {
        menu_item_string(model, index, "action")
    }

    fn submenu_with_label<M: IsA<gio::MenuModel>>(model: &M, label: &str) -> gio::MenuModel {
        for index in 0..model.n_items() {
            if menu_item_label(model, index) == label
                && let Some(submenu) = model.item_link(index, "submenu")
            {
                return submenu;
            }
        }
        panic!("missing submenu {label}");
    }

    fn section<M: IsA<gio::MenuModel>>(model: &M, index: i32) -> gio::MenuModel {
        model
            .item_link(index, "section")
            .unwrap_or_else(|| panic!("missing section {index}"))
    }

    fn section_items<M: IsA<gio::MenuModel>>(model: &M, index: i32) -> Vec<MenuItemSnapshot> {
        let section = section(model, index);
        direct_items(&section)
    }

    fn direct_items<M: IsA<gio::MenuModel>>(model: &M) -> Vec<MenuItemSnapshot> {
        (0..model.n_items())
            .map(|index| MenuItemSnapshot {
                label: menu_item_label(model, index),
                action: menu_item_action(model, index),
                submenu: model.item_link(index, "submenu").is_some(),
            })
            .collect()
    }

    #[test]
    fn status_save_label_matches_windows_contract() {
        let mut status = CurrentEditorStatus::default();
        assert_eq!(editor_save_state_label(&status), "No Save");

        status.can_save_as = true;
        assert_eq!(editor_save_state_label(&status), "Save As");

        status.can_save = true;
        assert_eq!(editor_save_state_label(&status), "Can Save");
    }

    #[test]
    fn window_title_matches_windows_status_format() {
        let mut status = CurrentEditorStatus::default();
        assert_eq!(editor_window_title(&status), "j3Text - j3Text");

        status.title = "Untitled".to_string();
        assert_eq!(editor_window_title(&status), "Untitled - j3Text");
    }

    #[test]
    fn status_detail_text_matches_windows_contract() {
        let mut status = CurrentEditorStatus::default();
        assert_eq!(editor_status_state_text(&status), "No file | j3Text");

        status.title = "Untitled".to_string();
        status.status_kind = crate::app::CurrentEditorStatusKind::Modified;
        assert_eq!(editor_status_state_text(&status), "Edited | Untitled");

        status.path = Some(PathBuf::from("/tmp/status-note.txt"));
        status.status_kind = crate::app::CurrentEditorStatusKind::ReadOnly;
        assert_eq!(
            editor_status_state_text(&status),
            "Read-only | /tmp/status-note.txt"
        );
    }

    #[test]
    fn status_column_uses_windows_utf16_offsets() {
        let text = "한😀x\r\nab";

        assert_eq!(line_and_utf16_column_from_char_offset(text, 0), (1, 1));
        assert_eq!(line_and_utf16_column_from_char_offset(text, 1), (1, 2));
        assert_eq!(
            line_and_utf16_column_from_char_offset(text, 2),
            (1, 4),
            "emoji before the caret occupies two Windows/Rich Edit UTF-16 columns"
        );
        assert_eq!(line_and_utf16_column_from_char_offset(text, 5), (2, 1));
        assert_eq!(line_and_utf16_column_from_char_offset(text, 7), (2, 3));
    }

    #[test]
    fn choice_dialog_button_specs_match_windows_message_boxes() {
        assert_eq!(
            YES_NO_BUTTONS,
            &[
                ("Yes", gtk::ResponseType::Yes),
                ("No", gtk::ResponseType::No)
            ]
        );
        assert_eq!(
            YES_NO_BUTTONS.first().map(|(_, response)| *response),
            Some(gtk::ResponseType::Yes)
        );
        assert_eq!(
            YES_NO_CANCEL_BUTTONS,
            &[
                ("Yes", gtk::ResponseType::Yes),
                ("No", gtk::ResponseType::No),
                ("Cancel", gtk::ResponseType::Cancel)
            ]
        );
        assert_eq!(
            YES_NO_CANCEL_BUTTONS.first().map(|(_, response)| *response),
            Some(gtk::ResponseType::Yes)
        );
        assert!(matches!(
            choice_from_dialog_response(gtk::ResponseType::None, YES_NO_BUTTONS),
            DialogChoice::No
        ));
        assert!(matches!(
            choice_from_dialog_response(gtk::ResponseType::None, YES_NO_CANCEL_BUTTONS),
            DialogChoice::Cancel
        ));
    }

    #[test]
    fn message_dialog_layout_matches_windows_title_and_body_contract() {
        let content =
            message_dialog_content("File Missing", "missing.txt not found.\n\nCreate it?");

        assert_eq!(
            content,
            MessageDialogContent {
                window_title: "File Missing",
                body_text: "missing.txt not found.\n\nCreate it?",
                secondary_text: None,
            }
        );
    }

    #[test]
    fn message_dialog_can_use_capture_dialog_as_transient_parent() {
        run_gtk_test_or_skip("GTK message dialog parent test", || {
            let parent = gtk::Window::new();
            let dialog = new_message_dialog(
                Some(&parent),
                "Shortcut",
                "Use Ctrl, Alt, or an F-key.",
                gtk::MessageType::Warning,
                gtk::ButtonsType::Ok,
            );
            let transient = dialog.transient_for();
            assert!(
                transient
                    .as_ref()
                    .is_some_and(|window| window.as_ptr() == parent.as_ptr()),
                "Windows owns shortcut capture warnings by the capture window"
            );
            dialog.close();
            parent.close();
        });
    }

    #[test]
    fn message_dialog_titles_match_windows_text() {
        assert_eq!(ERROR_DIALOG_TITLE, "j3Text Error");
        assert_eq!(FILE_CHANGED_DIALOG_TITLE, "File Changed");
        assert_eq!(ABOUT_DIALOG_TITLE, "About");
        assert_eq!(
            ABOUT_DIALOG_MESSAGE,
            concat!("j3Text ", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(ABOUT_DIALOG_URL, "https://github.com/edgarp9");
        assert_eq!(FIND_DIALOG_TITLE, "Find");
        assert_eq!(RESULTS_DIALOG_TITLE, "Results");
        assert_eq!(NO_MATCH_DIALOG_MESSAGE, "No match.");
        assert_eq!(SAVING_DIALOG_TITLE, "Saving");
        assert_eq!(
            SAVE_STILL_RUNNING_MESSAGE,
            "Save is still running. Please wait."
        );
        assert_eq!(SAVE_AS_OVERWRITE_DIALOG_TITLE, "Confirm Save As");
    }

    #[test]
    fn save_as_overwrite_confirmation_matches_windows_contract() {
        assert_eq!(
            save_as_overwrite_message(Path::new("/tmp/existing.txt")),
            "existing.txt already exists.\n\nDo you want to replace it?"
        );
        assert!(!save_target_needs_overwrite_confirmation(
            SaveTargetExpectation::Missing
        ));
        assert!(save_target_needs_overwrite_confirmation(
            SaveTargetExpectation::Unchanged(FileSnapshot {
                modified: None,
                byte_len: 4
            })
        ));
    }

    #[test]
    fn direct_save_uses_document_snapshot_expectation() {
        let path = PathBuf::from("/tmp/existing-note.txt");
        let snapshot = FileSnapshot {
            modified: Some(std::time::UNIX_EPOCH),
            byte_len: 17,
        };

        let expectation = save_target_expectation(
            Some(path.as_path()),
            path.as_path(),
            Some(snapshot),
            false,
            false,
            None,
        )
        .expect("build save expectation");

        assert_eq!(expectation, SaveTargetExpectation::Unchanged(snapshot));
    }

    #[test]
    fn encoding_dialog_validation_matches_windows_text() {
        assert_eq!(
            encoding_dialog_validation(" UTF_8 "),
            (Some(TextEncoding::Utf8), "OK: UTF-8".to_string())
        );
        assert_eq!(
            encoding_dialog_validation(""),
            (None, "Pick an encoding.".to_string())
        );
        assert_eq!(
            encoding_dialog_validation("bogus"),
            (None, "This encoding is not supported.".to_string())
        );
        assert_eq!(invalid_encoding_message(""), "Pick an encoding.");
        assert_eq!(
            invalid_encoding_message("bogus"),
            "\"bogus\" is not supported.\n\nTry UTF-8, CP949, or Windows-1252."
        );
    }

    #[test]
    fn visible_whitespace_size_limit_message_matches_windows_text() {
        assert_eq!(
            visible_whitespace_size_limit_message(VISIBLE_WHITESPACE_RENDER_LIMIT_BYTES + 1),
            "Marks work up to 25 MB.\n\nThis file is 25.0 MB, so normal view stays on."
        );
    }

    #[test]
    fn font_size_from_pango_matches_windows_rounding_and_limits() {
        assert_eq!(
            font_size_points_from_pango_size(pango_font_size_units(13), 11),
            13
        );
        assert_eq!(
            font_size_points_from_pango_size(12 * pango::SCALE + (pango::SCALE / 2), 11),
            13
        );
        assert_eq!(
            font_size_points_from_pango_size(2 * pango::SCALE, 11),
            MIN_FONT_SIZE_PT
        );
        assert_eq!(
            font_size_points_from_pango_size(500 * pango::SCALE, 11),
            MAX_FONT_SIZE_PT
        );
        assert_eq!(font_size_points_from_pango_size(0, 500), MAX_FONT_SIZE_PT);
    }

    #[test]
    fn oversized_document_load_disables_marks_without_persistence_boundary() {
        let mut app = EditorApp::new();
        let mut settings = EditorSettings {
            show_whitespace: true,
            ..EditorSettings::default()
        };
        app.set_settings(settings.clone());
        app.open_document(crate::domain::LoadedDocument {
            path: PathBuf::from("oversized-marks.txt"),
            content: "x".repeat(VISIBLE_WHITESPACE_RENDER_LIMIT_BYTES + 1),
            encoding: TextEncoding::Utf8,
            line_ending: LineEnding::Lf,
            snapshot: None,
            read_only_reason: None,
        });

        assert!(disable_visible_whitespace_for_oversized_current_document(
            &mut app
        ));
        assert!(!app.settings().show_whitespace);

        settings.show_whitespace = false;
        assert_eq!(app.settings(), &settings.sanitized());
    }

    #[test]
    fn gtk_key_capture_requires_safe_editor_shortcut() {
        let s = gdk::Key::from_name("s").expect("S key");
        let shortcut =
            shortcut_from_key_event(s, gdk::ModifierType::CONTROL_MASK).expect("Ctrl+S shortcut");
        assert_eq!(shortcut, KeyboardShortcut::CTRL_S);

        let a = gdk::Key::from_name("a").expect("A key");
        assert!(shortcut_from_key_event(a, gdk::ModifierType::SHIFT_MASK).is_none());

        let f3 = gdk::Key::from_name("F3").expect("F3 key");
        let shortcut =
            shortcut_from_key_event(f3, gdk::ModifierType::empty()).expect("F3 shortcut");
        assert_eq!(shortcut, KeyboardShortcut::F3);

        let keypad_one = gdk::Key::from_name("KP_1").expect("keypad 1 key");
        assert!(
            shortcut_from_key_event(keypad_one, gdk::ModifierType::CONTROL_MASK).is_none(),
            "Windows shortcut capture ignores numpad virtual keys"
        );
    }

    #[test]
    fn editor_shortcut_repeat_guard_releases_key_without_modifier_state() {
        let s = gdk::Key::from_name("s").expect("S key");
        let shortcut =
            shortcut_from_key_event(s, gdk::ModifierType::CONTROL_MASK).expect("Ctrl+S shortcut");
        let mut pressed = HashSet::new();

        assert!(pressed.insert(shortcut.key));
        let released_key = shortcut_key_from_gdk_key(s).expect("released key");
        pressed.remove(&released_key);

        assert!(
            pressed.insert(shortcut.key),
            "Windows uses the key-repeat bit instead of a persistent shortcut combo; Linux must not keep Ctrl+S blocked when Ctrl is released first"
        );
    }

    #[test]
    fn shortcut_menu_labels_reflect_current_binding() {
        assert_eq!(
            shortcut_current_label(Some(KeyboardShortcut::CTRL_S)),
            "Now: Ctrl+S"
        );
        assert_eq!(shortcut_current_label(None), "Now: Off");
        assert_eq!(
            shortcut_menu_title("Save", Some(KeyboardShortcut::CTRL_S)),
            "Save (Ctrl+S)"
        );
        assert_eq!(shortcut_menu_title("Save", None), "Save (Off)");
    }

    #[test]
    fn command_menu_labels_match_windows_shortcut_text() {
        assert_eq!(
            command_menu_label("Save", Some(KeyboardShortcut::CTRL_S)),
            "Save\tCtrl+S"
        );
        assert_eq!(command_menu_label("Save", None), "Save");
    }

    #[test]
    fn main_menu_top_level_matches_windows_order() {
        let app = EditorApp::new();
        let menu = build_menu_model(&app);

        let labels = (0..menu.n_items())
            .map(|index| menu_item_label(&menu, index))
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            [
                "File", "Edit", "Find", "View", "Tabs", "Text", "Settings", "Help"
            ]
        );
    }

    #[test]
    fn main_menu_sections_match_windows_menu_order() {
        let app = EditorApp::new();
        let root = build_menu_model(&app);

        let file = submenu_with_label(&root, "File");
        assert_eq!(
            section_items(&file, 0),
            [
                MenuItemSnapshot {
                    label: "New\tCtrl+N".to_string(),
                    action: Some("win.new".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Open...\tCtrl+O".to_string(),
                    action: Some("win.open".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Recent".to_string(),
                    action: None,
                    submenu: true,
                },
            ]
        );
        assert_eq!(
            section_items(&file, 1),
            [
                MenuItemSnapshot {
                    label: "Save\tCtrl+S".to_string(),
                    action: Some("win.save".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Save As...\tCtrl+Shift+S".to_string(),
                    action: Some("win.save-as".to_string()),
                    submenu: false,
                },
            ]
        );
        assert_eq!(
            section_items(&file, 2),
            [
                MenuItemSnapshot {
                    label: "Close\tCtrl+W".to_string(),
                    action: Some("win.close-tab".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Close All".to_string(),
                    action: Some("win.close-all-tabs".to_string()),
                    submenu: false,
                },
            ]
        );
        assert_eq!(
            section_items(&file, 3),
            [MenuItemSnapshot {
                label: "Exit".to_string(),
                action: Some("win.exit".to_string()),
                submenu: false,
            }]
        );

        let edit = submenu_with_label(&root, "Edit");
        assert_eq!(
            section_items(&edit, 0),
            [
                MenuItemSnapshot {
                    label: "Undo\tCtrl+Z".to_string(),
                    action: Some("win.undo".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Redo\tCtrl+Y".to_string(),
                    action: Some("win.redo".to_string()),
                    submenu: false,
                },
            ]
        );
        assert_eq!(
            section_items(&edit, 1),
            [
                MenuItemSnapshot {
                    label: "Cut\tCtrl+X".to_string(),
                    action: Some("win.cut".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Copy\tCtrl+C".to_string(),
                    action: Some("win.copy".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Paste\tCtrl+V".to_string(),
                    action: Some("win.paste".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Select All\tCtrl+A".to_string(),
                    action: Some("win.select-all".to_string()),
                    submenu: false,
                },
            ]
        );

        let find = submenu_with_label(&root, "Find");
        assert_eq!(
            section_items(&find, 0),
            [
                MenuItemSnapshot {
                    label: "Find...\tCtrl+F".to_string(),
                    action: Some("win.find".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Replace...\tCtrl+H".to_string(),
                    action: Some("win.replace".to_string()),
                    submenu: false,
                },
            ]
        );
        assert_eq!(
            section_items(&find, 1),
            [
                MenuItemSnapshot {
                    label: "Find Next\tF3".to_string(),
                    action: Some("win.find-next".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Find Prev\tShift+F3".to_string(),
                    action: Some("win.find-previous".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Find All\tCtrl+Shift+F".to_string(),
                    action: Some("win.find-all".to_string()),
                    submenu: false,
                },
            ]
        );

        let view = submenu_with_label(&root, "View");
        assert_eq!(
            section_items(&view, 0),
            [MenuItemSnapshot {
                label: "Commands...\tCtrl+Shift+P".to_string(),
                action: Some("win.command-palette".to_string()),
                submenu: false,
            }]
        );
        assert_eq!(
            section_items(&view, 1),
            [
                MenuItemSnapshot {
                    label: "Line Numbers".to_string(),
                    action: Some("win.line-numbers".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Marks".to_string(),
                    action: Some("win.visible-whitespace".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Word Wrap\tAlt+Z".to_string(),
                    action: Some("win.word-wrap".to_string()),
                    submenu: false,
                },
            ]
        );
        assert_eq!(
            section_items(&view, 2),
            [MenuItemSnapshot {
                label: "Theme".to_string(),
                action: None,
                submenu: true,
            }]
        );
    }

    #[test]
    fn remaining_main_menus_match_windows_menu_order() {
        let app = EditorApp::new();
        let root = build_menu_model(&app);

        let tabs = submenu_with_label(&root, "Tabs");
        assert_eq!(
            section_items(&tabs, 0),
            [
                MenuItemSnapshot {
                    label: "Move Left".to_string(),
                    action: Some("win.tab-left".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Move Right".to_string(),
                    action: Some("win.tab-right".to_string()),
                    submenu: false,
                },
            ]
        );
        assert_eq!(
            section_items(&tabs, 1),
            [
                MenuItemSnapshot {
                    label: "Close\tCtrl+W".to_string(),
                    action: Some("win.close-tab".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Close Others".to_string(),
                    action: Some("win.close-other-tabs".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Close All".to_string(),
                    action: Some("win.close-all-tabs".to_string()),
                    submenu: false,
                },
            ]
        );

        let text = submenu_with_label(&root, "Text");
        assert_eq!(
            section_items(&text, 0),
            [
                MenuItemSnapshot {
                    label: "Reopen Encoding...".to_string(),
                    action: Some("win.reopen-encoding".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Change Encoding...".to_string(),
                    action: Some("win.change-encoding".to_string()),
                    submenu: false,
                },
            ]
        );
        assert_eq!(
            section_items(&text, 1),
            [MenuItemSnapshot {
                label: "Line Ends".to_string(),
                action: None,
                submenu: true,
            }]
        );

        let settings = submenu_with_label(&root, "Settings");
        assert_eq!(
            section_items(&settings, 0),
            [
                MenuItemSnapshot {
                    label: "Font...".to_string(),
                    action: Some("win.choose-font".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Tab Size".to_string(),
                    action: None,
                    submenu: true,
                },
                MenuItemSnapshot {
                    label: "Shortcuts".to_string(),
                    action: None,
                    submenu: true,
                },
            ]
        );
        let help = submenu_with_label(&root, "Help");
        assert_eq!(
            direct_items(&help),
            [MenuItemSnapshot {
                label: "About".to_string(),
                action: Some("win.about".to_string()),
                submenu: false,
            }]
        );
    }

    #[test]
    fn nested_main_menu_items_match_windows_menu_order() {
        let app = EditorApp::new();
        let root = build_menu_model(&app);

        let file = submenu_with_label(&root, "File");
        let file_open = section(&file, 0);
        let recent = submenu_with_label(&file_open, "Recent");
        assert_eq!(
            direct_items(&recent),
            [MenuItemSnapshot {
                label: "(None)".to_string(),
                action: None,
                submenu: false,
            }]
        );

        let view = submenu_with_label(&root, "View");
        let theme_section = section(&view, 2);
        let theme = submenu_with_label(&theme_section, "Theme");
        assert_eq!(
            direct_items(&theme),
            [
                MenuItemSnapshot {
                    label: "System".to_string(),
                    action: Some("win.theme-system".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Light".to_string(),
                    action: Some("win.theme-light".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Classic Dark".to_string(),
                    action: Some("win.theme-classic-dark".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Sepia Teal".to_string(),
                    action: Some("win.theme-sepia-teal".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Graphite".to_string(),
                    action: Some("win.theme-graphite".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Forest".to_string(),
                    action: Some("win.theme-forest".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Steel Blue".to_string(),
                    action: Some("win.theme-steel-blue".to_string()),
                    submenu: false,
                },
            ]
        );

        let text = submenu_with_label(&root, "Text");
        let line_end_section = section(&text, 1);
        let line_ends = submenu_with_label(&line_end_section, "Line Ends");
        assert_eq!(
            direct_items(&line_ends),
            [
                MenuItemSnapshot {
                    label: "CRLF".to_string(),
                    action: Some("win.line-ending-crlf".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "LF".to_string(),
                    action: Some("win.line-ending-lf".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "CR".to_string(),
                    action: Some("win.line-ending-cr".to_string()),
                    submenu: false,
                },
            ]
        );

        let settings = submenu_with_label(&root, "Settings");
        let settings_main = section(&settings, 0);
        let tab_size = submenu_with_label(&settings_main, "Tab Size");
        assert_eq!(
            direct_items(&tab_size),
            [
                MenuItemSnapshot {
                    label: "2 spaces".to_string(),
                    action: Some("win.tab-size-2".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "4 spaces".to_string(),
                    action: Some("win.tab-size-4".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "8 spaces".to_string(),
                    action: Some("win.tab-size-8".to_string()),
                    submenu: false,
                },
            ]
        );

        let shortcuts = submenu_with_label(&settings_main, "Shortcuts");
        assert_eq!(
            shortcuts.n_items(),
            EditorCommandId::SHORTCUT_COMMANDS.len() as i32
        );
        for (index, command) in EditorCommandId::SHORTCUT_COMMANDS
            .iter()
            .copied()
            .enumerate()
        {
            let current = app.settings().shortcuts.shortcut_for(command);
            let default = command.default_shortcut();
            let title = command.shortcut_title().unwrap_or("Command");
            let command_menu = submenu_with_label(&shortcuts, &shortcut_menu_title(title, current));
            assert_eq!(
                section_items(&command_menu, 0),
                [MenuItemSnapshot {
                    label: shortcut_current_label(current),
                    action: None,
                    submenu: false,
                }]
            );
            assert_eq!(
                section_items(&command_menu, 1),
                [
                    MenuItemSnapshot {
                        label: "Set...".to_string(),
                        action: Some(format!("win.shortcut-{index}-capture")),
                        submenu: false,
                    },
                    MenuItemSnapshot {
                        label: format!("Default: {}", optional_shortcut_display_name(default)),
                        action: Some(format!("win.shortcut-{index}-default")),
                        submenu: false,
                    },
                    MenuItemSnapshot {
                        label: "Off".to_string(),
                        action: Some(format!("win.shortcut-{index}-disable")),
                        submenu: false,
                    },
                ]
            );
        }
    }

    #[test]
    fn recent_menu_caps_items_to_windows_limit() {
        let mut app = EditorApp::new();
        let recent_files = (0..(crate::domain::MAX_RECENT_FILES + 2))
            .map(|index| PathBuf::from(format!("recent-{index}.txt")))
            .collect::<Vec<_>>();
        app.set_recent_files(recent_files);
        let root = build_menu_model(&app);

        let file = submenu_with_label(&root, "File");
        let file_open = section(&file, 0);
        let recent = submenu_with_label(&file_open, "Recent");
        let items = direct_items(&recent);

        assert_eq!(items.len(), crate::domain::MAX_RECENT_FILES);
        for (index, item) in items.iter().enumerate() {
            assert_eq!(
                item,
                &MenuItemSnapshot {
                    label: format!("{} recent-{index}.txt", index + 1),
                    action: Some(format!("win.recent-{index}")),
                    submenu: false,
                }
            );
        }
    }

    #[test]
    fn context_menus_match_windows_menu_order() {
        let app = EditorApp::new();

        assert_eq!(
            section_items(&build_editor_context_menu_model(&app), 0),
            [
                MenuItemSnapshot {
                    label: "Undo\tCtrl+Z".to_string(),
                    action: Some("win.undo".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Redo\tCtrl+Y".to_string(),
                    action: Some("win.redo".to_string()),
                    submenu: false,
                },
            ]
        );
        assert_eq!(
            section_items(&build_editor_context_menu_model(&app), 1),
            [
                MenuItemSnapshot {
                    label: "Cut\tCtrl+X".to_string(),
                    action: Some("win.cut".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Copy\tCtrl+C".to_string(),
                    action: Some("win.copy".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Paste\tCtrl+V".to_string(),
                    action: Some("win.paste".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Select All\tCtrl+A".to_string(),
                    action: Some("win.select-all".to_string()),
                    submenu: false,
                },
            ]
        );

        let mut single_tab_app = EditorApp::new();
        single_tab_app.new_document();
        let single_tab_context = build_tab_context_menu_model(&single_tab_app);
        assert_eq!(
            section_items(&single_tab_context, 0),
            [MenuItemSnapshot {
                label: "Open in New Window".to_string(),
                action: None,
                submenu: false,
            }]
        );

        let mut multi_tab_app = EditorApp::new();
        multi_tab_app.new_document();
        multi_tab_app.new_document();
        let multi_tab_context = build_tab_context_menu_model(&multi_tab_app);
        assert_eq!(
            section_items(&multi_tab_context, 0),
            [MenuItemSnapshot {
                label: "Open in New Window".to_string(),
                action: Some("win.open-new-window".to_string()),
                submenu: false,
            }]
        );
        assert_eq!(
            section_items(&multi_tab_context, 1),
            [
                MenuItemSnapshot {
                    label: "Close\tCtrl+W".to_string(),
                    action: Some("win.close-tab".to_string()),
                    submenu: false,
                },
                MenuItemSnapshot {
                    label: "Close Others".to_string(),
                    action: Some("win.close-other-tabs".to_string()),
                    submenu: false,
                },
            ]
        );
    }

    #[test]
    fn programmatic_buffer_load_clears_undo_history() {
        run_gtk_test_or_skip("GTK TextBuffer undo test", || {
            let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
            buffer.set_enable_undo(true);
            buffer.set_text("old");
            set_buffer_text_without_undo(&buffer, "loaded");

            assert_eq!(
                buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), true)
                    .as_str(),
                "loaded"
            );
            assert!(!buffer.can_undo());

            let mut end = buffer.end_iter();
            buffer.insert(&mut end, "!");
            assert!(buffer.can_undo());
        });
    }

    #[test]
    fn gtk_text_buffer_preserves_line_endings_for_domain_sync() {
        run_gtk_test_or_skip("GTK TextBuffer line ending test", || {
            let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
            let text = "crlf\r\ncr\rlf\n";
            set_buffer_text_without_undo(&buffer, text);
            let roundtrip = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), true)
                .to_string();

            assert_eq!(roundtrip, text);
            let scan = LoadedTextAnalysis::scan_text(&roundtrip);
            assert_eq!(scan.analysis.line_ending, LineEnding::Crlf);
        });
    }

    #[test]
    fn file_dialog_filters_match_windows_order() {
        let filters = file_dialog_filter_specs();
        assert_eq!(filters.len(), 2);
        assert_eq!(filters[0].name, "Text (*.txt)");
        assert_eq!(filters[0].pattern, "*.txt");
        assert_eq!(filters[1].name, "All (*.*)");
        assert_eq!(filters[1].pattern, "*");
    }

    #[test]
    fn tab_hit_test_uses_half_open_tab_bounds() {
        assert!(point_in_rect(10.0, 20.0, 10.0, 20.0, 100.0, 30.0));
        assert!(point_in_rect(109.999, 49.999, 10.0, 20.0, 100.0, 30.0));
        assert!(!point_in_rect(110.0, 25.0, 10.0, 20.0, 100.0, 30.0));
        assert!(!point_in_rect(30.0, 50.0, 10.0, 20.0, 100.0, 30.0));
        assert!(!point_in_rect(9.999, 25.0, 10.0, 20.0, 100.0, 30.0));
        assert!(!point_in_rect(30.0, 19.999, 10.0, 20.0, 100.0, 30.0));
    }

    #[test]
    fn tab_hit_test_rejects_empty_bounds() {
        assert!(!point_in_rect(10.0, 20.0, 10.0, 20.0, 0.0, 30.0));
        assert!(!point_in_rect(10.0, 20.0, 10.0, 20.0, 100.0, 0.0));
    }

    #[test]
    fn system_theme_resolves_like_windows_contract() {
        assert_eq!(
            resolve_theme_mode(ThemeMode::System, false),
            ThemeMode::Light
        );
        assert_eq!(
            resolve_theme_mode(ThemeMode::System, true),
            ThemeMode::ClassicDark
        );
        assert_eq!(
            resolve_theme_mode(ThemeMode::SteelBlue, true),
            ThemeMode::SteelBlue
        );
    }

    #[test]
    fn gtk_theme_name_dark_detection_uses_theme_token() {
        assert!(gtk_theme_name_prefers_dark("Adwaita-dark"));
        assert!(gtk_theme_name_prefers_dark("Breeze-Dark"));
        assert!(!gtk_theme_name_prefers_dark("Adwaita"));
        assert!(!gtk_theme_name_prefers_dark("Darkula"));
    }

    #[test]
    fn tab_width_uses_font_metric_char_width() {
        assert_eq!(tab_width_pixels(4, 9 * pango::SCALE), 36);
        assert_eq!(tab_width_pixels(2, 11 * pango::SCALE), 22);
        assert_eq!(
            tab_width_pixels(8, (7 * pango::SCALE) + (pango::SCALE / 2)),
            60
        );
    }

    #[test]
    fn tab_width_has_safe_fallback_for_invalid_metric() {
        assert_eq!(tab_width_pixels(4, 0), 4);
        assert_eq!(tab_width_pixels(0, 12 * pango::SCALE), 12);
    }
}
