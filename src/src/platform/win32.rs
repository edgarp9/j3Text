use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::ffi::c_void;
use std::fmt::Write as _;
use std::mem::{self, MaybeUninit, size_of};
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Once, OnceLock};
use std::thread;

use windows_sys::Win32::Foundation::{
    COLORREF, ERROR_SUCCESS, FARPROC, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT,
    SetLastError, WPARAM,
};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::{
    ANSI_FIXED_FONT, COLOR_BTNFACE, CreateFontW, CreateSolidBrush, DT_END_ELLIPSIS, DT_SINGLELINE,
    DT_VCENTER, DeleteObject, DrawTextW, FillRect, GetDC, GetDeviceCaps, GetStockObject,
    GetSysColorBrush, HBRUSH, HDC, HFONT, InvalidateRect, LOGFONTW, LOGPIXELSY, ReleaseDC,
    ScreenToClient, SetBkColor, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleHandleW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExA,
};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Controls::Dialogs::{
    CF_FORCEFONTEXIST, CF_INITTOLOGFONTSTRUCT, CF_LIMITSIZE, CF_NOSCRIPTSEL, CF_NOSTYLESEL,
    CF_NOVERTFONTS, CF_SCREENFONTS, CHOOSEFONTW, ChooseFontW, CommDlgExtendedError,
    GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY,
    OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Controls::{
    CDDS_ITEMPREPAINT, CDDS_PREPAINT, CDRF_NEWFONT, CDRF_NOTIFYITEMDRAW, CLR_DEFAULT,
    DRAWITEMSTRUCT, EM_GETFIRSTVISIBLELINE, EM_GETLINECOUNT, EM_GETSEL, EM_LINEFROMCHAR,
    EM_LINEINDEX, EM_LINESCROLL, EM_SCROLLCARET, EM_SETSEL, ICC_BAR_CLASSES, ICC_TAB_CLASSES,
    INITCOMMONCONTROLSEX, InitCommonControlsEx, NM_CUSTOMDRAW, NM_RCLICK, NMCUSTOMDRAW,
    NMTTDISPINFOW, SB_SETBKCOLOR, SB_SETPARTS, SB_SETTEXTW, SBT_OWNERDRAW, SetWindowTheme,
    TASKDIALOG_BUTTON, TASKDIALOGCONFIG, TCHITTESTINFO, TCIF_TEXT, TCITEMW, TCM_DELETEALLITEMS,
    TCM_GETCURSEL, TCM_GETTOOLTIPS, TCM_HITTEST, TCM_INSERTITEMW, TCM_SETCURSEL, TCM_SETITEMW,
    TCN_SELCHANGE, TCS_TOOLTIPS, TD_ERROR_ICON, TDF_ALLOW_DIALOG_CANCELLATION, TTM_SETMAXTIPWIDTH,
    TTN_GETDISPINFOW, TaskDialogIndirect,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled, SetFocus};
use windows_sys::Win32::UI::Shell::{
    DragAcceptFiles, DragFinish, DragQueryFileW, HDROP, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, BS_PUSHBUTTON, CREATESTRUCTW, CW_USEDEFAULT, CallNextHookEx, CreateMenu,
    CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW,
    DrawMenuBar, EN_CHANGE, EN_HSCROLL, EN_KILLFOCUS, EN_SETFOCUS, EN_VSCROLL, EnableMenuItem,
    GWL_STYLE, GWLP_USERDATA, GetClientRect, GetCursorPos, GetMenu, GetMessageW, GetWindowLongPtrW,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, HCBT_ACTIVATE, HHOOK, HICON, HMENU,
    IDC_ARROW, IDNO, IDYES, IsChild, IsDialogMessageW, IsWindow, KillTimer, LoadCursorW, LoadIconW,
    MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_YESNO, MB_YESNOCANCEL, MENUINFO,
    MF_BYCOMMAND, MF_CHECKED, MF_ENABLED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    MF_UNCHECKED, MIM_APPLYTOSUBMENUS, MIM_BACKGROUND, MSG, MessageBoxW, MoveWindow,
    PostQuitMessage, RegisterClassExW, SC_CLOSE, SW_HIDE, SW_SHOW, SW_SHOWDEFAULT, SW_SHOWNORMAL,
    SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetMenu, SetMenuInfo, SetTimer,
    SetWindowLongPtrW, SetWindowPos, SetWindowTextW, SetWindowsHookExW, ShowWindow, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, UnhookWindowsHookEx, WH_CBT,
    WINDOW_EX_STYLE, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_COPY, WM_CREATE, WM_CTLCOLORBTN,
    WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_CUT, WM_DESTROY, WM_DPICHANGED,
    WM_DRAWITEM, WM_DROPFILES, WM_ENTERSIZEMOVE, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_KEYDOWN,
    WM_KEYUP, WM_LBUTTONUP, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_NOTIFY, WM_SETTINGCHANGE,
    WM_SIZE, WM_SYSCOMMAND, WM_SYSKEYDOWN, WM_THEMECHANGED, WM_TIMER, WM_UNDO, WM_VSCROLL,
    WNDCLASSEXW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
    WS_EX_CLIENTEDGE, WS_HSCROLL, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_SYSMENU, WS_TABSTOP,
    WS_VISIBLE, WS_VSCROLL,
};

use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState;

use crate::app::{CurrentEditorStatus, CurrentEditorStatusKind, EditorApp, EditorSurfaceState};
use crate::domain::{
    DocumentId, DocumentMetrics, EditorCommand, EditorCommandId, EditorSettings, FileSnapshot,
    KeyboardShortcut, LineEnding, MAX_DOCUMENT_LOAD_BYTES, MAX_FONT_SIZE_PT, MIN_FONT_SIZE_PT,
    ReadOnlyReason, SearchDirection, ShortcutKey, TextEncoding, ThemeMode,
    VISIBLE_WHITESPACE_RENDER_LIMIT_BYTES, all_commands, can_load_document_bytes,
    can_render_visible_whitespace_bytes, find_text, render_visible_whitespace_for_display,
    should_warn_large_file,
};
use crate::error::{AppError, FileAccessKind, PlatformErrorKind};
use crate::infra::{
    FileDocumentIo, MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES, SaveTargetExpectation, SavedFileSnapshot,
    UserDataStore,
};
use crate::notices::about_text;

use super::last_win32_error;

const CLASS_NAME: &str = "J3TextMainWindow";
const ABOUT_DIALOG_CLASS_NAME: &str = "J3TextAboutDialog";
const RICH_EDIT_CLASS: &str = "RICHEDIT50W";
const RICH_EDIT_DLL: &[u8] = b"Msftedit.dll\0";
const APP_TITLE: &str = "j3Text";
const ABOUT_DIALOG_TITLE: &str = "About j3Text";
const ABOUT_DIALOG_VERSION_LABEL: &str = concat!("j3Text ", env!("CARGO_PKG_VERSION"));
const ABOUT_DIALOG_URL: &str = "https://github.com/edgarp9";
const ABOUT_DIALOG_WIDTH: i32 = 450;
const ABOUT_DIALOG_HEIGHT: i32 = 400;
const ABOUT_BODY_SCROLL_WIDTH: i32 = 402;
const ABOUT_BODY_SCROLL_HEIGHT: i32 = 250;
const ABOUT_BUTTON_TOP: i32 = 316;
const ABOUT_DIALOG_STYLE: u32 = WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
const ID_APP_ICON: u16 = 1;
const TIMER_STATUS: usize = 1;
const TIMER_STATUS_INTERVAL_MS: u32 = 250;
const PERSISTENCE_DEBOUNCE_TICKS: u8 = 2;
const EXTERNAL_CHANGE_RELOAD_BUTTON: i32 = 101;
const EXTERNAL_CHANGE_SAVE_AS_BUTTON: i32 = 102;
const EXTERNAL_CHANGE_CANCEL_BUTTON: i32 = 103;

const TAB_HEIGHT: i32 = 28;
const TAB_TOOLTIP_MAX_WIDTH: i32 = 720;
const TAB_TOOLTIP_WRAP_COLUMN: usize = 80;
const FIND_BAR_HEIGHT: i32 = 34;
const SEARCH_RESULTS_HEIGHT: i32 = 112;
const COMMAND_PALETTE_HEIGHT: i32 = 154;
const STATUS_HEIGHT: i32 = 24;
const LINE_NUMBER_WIDTH: i32 = 58;
const DEFAULT_WINDOW_WIDTH: i32 = 600;
const DEFAULT_WINDOW_HEIGHT: i32 = 500;

thread_local! {
    static CENTERED_DIALOG_OWNER: Cell<HWND> = const { Cell::new(null_mut()) };
    static ACTIVE_MODAL_DIALOG_DEPTH: Cell<u32> = const { Cell::new(0) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DpiMetrics {
    dpi_y: i32,
}

impl Default for DpiMetrics {
    fn default() -> Self {
        Self {
            dpi_y: DEFAULT_DPI as i32,
        }
    }
}

impl DpiMetrics {
    fn new(dpi_y: i32) -> Self {
        let dpi_y = if dpi_y > 0 { dpi_y } else { DEFAULT_DPI as i32 };
        Self { dpi_y }
    }

    fn for_window(hwnd: HWND) -> Self {
        Self::new(dpi_y_for_window(hwnd))
    }

    fn from_wm_dpi_changed(wparam: WPARAM) -> Option<Self> {
        let dpi_y = hiword(wparam) as i32;
        (dpi_y > 0).then(|| Self::new(dpi_y))
    }

    fn ui_scale(self) -> UiScale {
        UiScale { dpi: self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UiScale {
    dpi: DpiMetrics,
}

impl UiScale {
    fn px(self, value: i32) -> i32 {
        scale_px_for_dpi(value, self.dpi.dpi_y)
    }

    fn font_height(self, point_size: u32) -> i32 {
        points_to_logical_height(point_size, self.dpi.dpi_y)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SizeMoveDpiState {
    in_loop: bool,
    dpi_changed: bool,
    status_timer_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SizeMoveDpiExit {
    dpi_changed: bool,
    status_timer_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatusTimerAction {
    Run,
    DeferUntilSizeMoveExit,
    SkipDuringModalDialog,
}

impl SizeMoveDpiState {
    fn enter(&mut self) {
        self.in_loop = true;
        self.dpi_changed = false;
        self.status_timer_pending = false;
    }

    fn mark_dpi_changed(&mut self) {
        self.dpi_changed = true;
    }

    fn defer_status_timer(&mut self) {
        self.status_timer_pending = true;
    }

    fn exit(&mut self) -> SizeMoveDpiExit {
        let exit = SizeMoveDpiExit {
            dpi_changed: self.dpi_changed,
            status_timer_pending: self.status_timer_pending,
        };
        *self = Self::default();
        exit
    }
}

fn status_timer_action(
    size_move: SizeMoveDpiState,
    modal_dialog_active: bool,
) -> StatusTimerAction {
    if size_move.in_loop {
        StatusTimerAction::DeferUntilSizeMoveExit
    } else if modal_dialog_active {
        StatusTimerAction::SkipDuringModalDialog
    } else {
        StatusTimerAction::Run
    }
}

const ID_FILE_NEW: u16 = 1001;
const ID_FILE_OPEN: u16 = 1002;
const ID_FILE_SAVE: u16 = 1003;
const ID_FILE_SAVE_AS: u16 = 1004;
const ID_FILE_CLOSE_TAB: u16 = 1005;
const ID_FILE_CLOSE_OTHER_TABS: u16 = 1006;
const ID_FILE_CLOSE_ALL_TABS: u16 = 1007;
const ID_FILE_EXIT: u16 = 1008;
const ID_FILE_RECENT_BASE: u16 = 1050;

const ID_EDIT_FIND: u16 = 1101;
const ID_EDIT_REPLACE: u16 = 1102;
const ID_EDIT_FIND_NEXT: u16 = 1103;
const ID_EDIT_FIND_PREVIOUS: u16 = 1104;
const ID_EDIT_FIND_ALL: u16 = 1105;
const ID_EDIT_UNDO: u16 = 1106;
const ID_EDIT_CUT: u16 = 1107;
const ID_EDIT_COPY: u16 = 1108;
const ID_EDIT_PASTE: u16 = 1109;
const ID_EDIT_SELECT_ALL: u16 = 1110;
const ID_EDIT_REDO: u16 = 1111;

const ID_VIEW_LINE_NUMBERS: u16 = 1201;
const ID_VIEW_WHITESPACE: u16 = 1202;
const ID_VIEW_COMMAND_PALETTE: u16 = 1203;

const ID_ENCODING_REOPEN: u16 = 1301;
const ID_ENCODING_CONVERT: u16 = 1302;

const ID_LINE_ENDING_CRLF: u16 = 1401;
const ID_LINE_ENDING_LF: u16 = 1402;
const ID_LINE_ENDING_CR: u16 = 1403;

const ID_SETTINGS_CHOOSE_FONT: u16 = 1501;
const ID_SETTINGS_TAB_SIZE_2: u16 = 1522;
const ID_SETTINGS_TAB_SIZE_4: u16 = 1524;
const ID_SETTINGS_TAB_SIZE_8: u16 = 1528;
const ID_SETTINGS_WORD_WRAP: u16 = 1531;
const ID_THEME_SYSTEM: u16 = 1541;
const ID_THEME_LIGHT: u16 = 1542;
const ID_THEME_CLASSIC_DARK: u16 = 1543;
const ID_THEME_SEPIA_TEAL: u16 = 1544;
const ID_THEME_GRAPHITE: u16 = 1545;
const ID_THEME_FOREST: u16 = 1546;
const ID_THEME_STEEL_BLUE: u16 = 1547;
const ID_HELP_ABOUT: u16 = 1601;
const ID_TAB_MOVE_LEFT: u16 = 1701;
const ID_TAB_MOVE_RIGHT: u16 = 1702;
const ID_TAB_OPEN_NEW_WINDOW: u16 = 1703;
const ID_SETTINGS_SHORTCUT_BASE: u16 = 1900;
const SHORTCUT_MENU_ACTION_COUNT: u16 = 3;

const ID_FIND_TEXT: u16 = 2101;
const ID_REPLACE_TEXT: u16 = 2102;
const ID_FIND_NEXT_BUTTON: u16 = 2103;
const ID_FIND_PREV_BUTTON: u16 = 2104;
const ID_REPLACE_BUTTON: u16 = 2105;
const ID_REPLACE_ALL_BUTTON: u16 = 2106;
const ID_FIND_CLOSE_BUTTON: u16 = 2107;
const ID_FIND_ALL_BUTTON: u16 = 2108;
const ID_SEARCH_RESULTS_LIST: u16 = 2109;
const ID_COMMAND_FILTER: u16 = 2201;
const ID_COMMAND_LIST: u16 = 2202;
const ID_SHORTCUT_CAPTURE_CANCEL: u16 = 2301;
const ID_ABOUT_OPEN_URL: u16 = 2401;
const ID_ABOUT_OK: u16 = 2402;
const ID_ENCODING_COMBO: u16 = 3001;
const ID_ENCODING_STATUS: u16 = 3002;
const ID_ENCODING_OK: u16 = 3003;
const ID_ENCODING_CANCEL: u16 = 3004;

const ES_LEFT: u32 = 0x0000;
const ES_RIGHT: u32 = 0x0002;
const ES_MULTILINE: u32 = 0x0004;
const ES_AUTOVSCROLL: u32 = 0x0040;
const ES_AUTOHSCROLL: u32 = 0x0080;
const ES_NOHIDESEL: u32 = 0x0100;
const ES_READONLY: u32 = 0x0800;
const SS_CENTERIMAGE: u32 = 0x0200;
const WM_SETFONT_LOCAL: u32 = 0x0030;
const EM_GETMODIFY_LOCAL: u32 = 0x00B8;
const EM_SETMODIFY_LOCAL: u32 = 0x00B9;
const EM_CANUNDO_LOCAL: u32 = 0x00C6;
const EM_SETREADONLY_LOCAL: u32 = 0x00CF;
const EM_SETTABSTOPS_LOCAL: u32 = 0x00CB;
const WM_USER_LOCAL: u32 = 0x0400;
const EM_EXLIMITTEXT_LOCAL: u32 = WM_USER_LOCAL + 53;
const EM_PASTESPECIAL_LOCAL: u32 = WM_USER_LOCAL + 64;
const EM_SETBKGNDCOLOR_LOCAL: u32 = WM_USER_LOCAL + 67;
const EM_SETCHARFORMAT_LOCAL: u32 = WM_USER_LOCAL + 68;
const EM_SETEVENTMASK_LOCAL: u32 = WM_USER_LOCAL + 69;
const EM_SETTARGETDEVICE_LOCAL: u32 = WM_USER_LOCAL + 72;
const EM_REDO_LOCAL: u32 = WM_USER_LOCAL + 84;
const EM_SETTEXTMODE_LOCAL: u32 = WM_USER_LOCAL + 89;
const EM_GETTEXTEX_LOCAL: u32 = WM_USER_LOCAL + 94;
const EM_GETTEXTLENGTHEX_LOCAL: u32 = WM_USER_LOCAL + 95;
const EM_SETTEXTEX_LOCAL: u32 = WM_USER_LOCAL + 97;
const EM_CANREDO_LOCAL: u32 = WM_USER_LOCAL + 85;
const ENM_CHANGE_LOCAL: LPARAM = 0x00000001;
const ENM_SCROLL_LOCAL: LPARAM = 0x00000004;
const ENM_SELCHANGE_LOCAL: LPARAM = 0x00080000;
const EN_SELCHANGE_LOCAL: u32 = 0x0702;
const CP_UTF8_LOCAL: u32 = 65001;
const CP_UNICODE_LOCAL: u32 = 1200;
const CF_UNICODETEXT_LOCAL: WPARAM = 13;
const GT_USECRLF_LOCAL: u32 = 0x0001;
const GTL_USECRLF_LOCAL: u32 = 0x0001;
const GTL_PRECISE_LOCAL: u32 = 0x0002;
const GTL_NUMCHARS_LOCAL: u32 = 0x0008;
const GTL_NUMBYTES_LOCAL: u32 = 0x0010;
const ST_KEEPUNDO_LOCAL: u32 = 0x0001;
const ST_SELECTION_LOCAL: u32 = 0x0002;
const ST_UNICODE_LOCAL: u32 = 0x0008;
const ST_PLAINTEXTONLY_LOCAL: u32 = 0x0020;
const TM_PLAINTEXT_LOCAL: u32 = 1;
const TM_MULTILEVELUNDO_LOCAL: u32 = 8;
const TM_MULTICODEPAGE_LOCAL: u32 = 32;
const RICH_EDIT_TEXT_MODE: WPARAM =
    (TM_PLAINTEXT_LOCAL | TM_MULTILEVELUNDO_LOCAL | TM_MULTICODEPAGE_LOCAL) as WPARAM;
const RICH_EDIT_EDITABLE_TEXT_LIMIT: LPARAM = MAX_DOCUMENT_LOAD_BYTES as LPARAM;
const RICH_EDIT_SURFACE_TEXT_LIMIT: LPARAM = MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES as LPARAM;
const RICH_EDIT_SURFACE_TEXT_LIMIT_UNITS: usize = MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES as usize;
const EDITOR_DOCUMENT_TEXT_TOO_LARGE_MESSAGE: &str = "Text is too large.";
const REPLACE_ALL_RESULT_TEXT_LENGTH_OVERFLOW_MESSAGE: &str = "New text is too large.";
const RICH_EDIT_SURFACE_TEXT_TOO_LARGE_MESSAGE: &str = "File is too large to show.";
const CFM_COLOR_LOCAL: u32 = 0x40000000;
const SCF_DEFAULT_LOCAL: WPARAM = 0x0000;
const SCF_ALL_LOCAL: WPARAM = 0x0004;
const LF_FACESIZE_LOCAL: usize = 32;
const LBS_NOTIFY: u32 = 0x0001;
const LB_ADDSTRING: u32 = 0x0180;
const LB_SETCURSEL: u32 = 0x0186;
const LB_GETCURSEL: u32 = 0x0188;
const LB_RESETCONTENT: u32 = 0x0184;
const LBN_DBLCLK: u16 = 2;
const LB_ERR: isize = -1;
const LB_ERRSPACE: isize = -2;
const CBS_DROPDOWN: u32 = 0x0002;
const CBS_AUTOHSCROLL: u32 = 0x0040;
const CBS_HASSTRINGS: u32 = 0x0200;
const CB_LIMITTEXT: u32 = 0x0141;
const CB_ADDSTRING: u32 = 0x0143;
const CB_SETCURSEL: u32 = 0x014E;
const CB_ERR: isize = -1;
const CB_ERRSPACE: isize = -2;
const CBN_SELCHANGE: u16 = 1;
const CBN_EDITCHANGE: u16 = 5;
const VK_CONTROL_CODE: i32 = 0x11;
const VK_SHIFT_CODE: i32 = 0x10;
const VK_MENU_CODE: i32 = 0x12;
const VK_RETURN_CODE: u32 = 0x0D;
const VK_ESCAPE_CODE: u32 = 0x1B;
const VK_SPACE_CODE: u32 = 0x20;
const VK_F1_CODE: u32 = 0x70;
const VK_F24_CODE: u32 = 0x87;

struct ConfirmedLargeFilePolicy {
    read_only_reason: Option<ReadOnlyReason>,
    byte_len: u64,
}

#[repr(C)]
struct RichEditCharFormatW {
    cb_size: u32,
    mask: u32,
    effects: u32,
    height: i32,
    offset: i32,
    text_color: COLORREF,
    charset: u8,
    pitch_and_family: u8,
    face_name: [u16; LF_FACESIZE_LOCAL],
}

#[repr(C)]
struct RichEditGetTextEx {
    cb: u32,
    flags: u32,
    codepage: u32,
    default_char: *const u8,
    used_default_char: *mut i32,
}

#[repr(C)]
struct RichEditGetTextLengthEx {
    flags: u32,
    codepage: u32,
}

#[repr(C)]
struct RichEditSetTextEx {
    flags: u32,
    codepage: u32,
}

struct PendingSave {
    document_id: DocumentId,
    path: PathBuf,
    encoding: TextEncoding,
    line_ending: LineEnding,
    receiver: Receiver<Result<SavedFileSnapshot, AppError>>,
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::domain::utf16_offset_to_byte_index;

    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn new(prefix: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock should be after unix epoch")
                .as_nanos();
            let path = env::temp_dir().join(format!("{prefix}-{nonce}"));
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
    fn missing_startup_path_save_requires_missing_target() {
        let path = PathBuf::from("C:\\Temp\\new-note.txt");

        let expectation = save_target_expectation(
            Some(path.as_path()),
            path.as_path(),
            None,
            true,
            false,
            None,
        )
        .expect("build save expectation");

        assert_eq!(expectation, SaveTargetExpectation::Missing);
    }

    #[test]
    fn direct_save_uses_document_snapshot_expectation() {
        let path = PathBuf::from("C:\\Temp\\existing-note.txt");
        let snapshot = FileSnapshot {
            modified: Some(UNIX_EPOCH),
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
    fn selected_save_target_missing_requires_missing_target() {
        let root = TempRoot::new("j3text-save-as-missing-target");
        let io = FileDocumentIo::new();
        let path = root.path().join("selected.txt");

        let expectation =
            selected_save_target_expectation(&io, &path).expect("capture selected target");

        assert_eq!(expectation, SaveTargetExpectation::Missing);
    }

    #[test]
    fn selected_save_target_existing_requires_unchanged_target() {
        let root = TempRoot::new("j3text-save-as-existing-target");
        let io = FileDocumentIo::new();
        let path = root.path().join("selected.txt");
        fs::write(&path, b"selected target\r\n").expect("write selected target");
        let snapshot = io.file_snapshot(&path).expect("snapshot selected target");
        let metadata = io
            .file_metadata_snapshot(&path)
            .expect("metadata selected target");

        let expectation =
            selected_save_target_expectation(&io, &path).expect("capture selected target");

        assert_eq!(
            expectation,
            SaveTargetExpectation::UnchangedWithMetadata { snapshot, metadata }
        );
    }

    #[test]
    fn selected_save_target_large_existing_uses_metadata_expectation() {
        let root = TempRoot::new("j3text-save-as-large-existing-target");
        let io = FileDocumentIo::new();
        let path = root.path().join("selected.txt");
        let file = fs::File::create(&path).expect("create selected target");
        file.set_len(MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES + 1)
            .expect("extend selected target");
        let metadata = io
            .file_metadata_snapshot(&path)
            .expect("metadata selected target");

        let expectation =
            selected_save_target_expectation(&io, &path).expect("capture selected target");

        assert_eq!(
            expectation,
            SaveTargetExpectation::UnchangedMetadata(metadata)
        );
    }

    #[test]
    fn different_save_path_uses_selected_target_expectation() {
        let document_path = PathBuf::from("C:\\Temp\\original.txt");
        let save_path = PathBuf::from("C:\\Temp\\selected.txt");
        let selected_snapshot = FileSnapshot {
            modified: Some(UNIX_EPOCH),
            byte_len: 17,
        };

        let expectation = save_target_expectation(
            Some(document_path.as_path()),
            save_path.as_path(),
            None,
            false,
            true,
            Some(SaveTargetExpectation::Unchanged(selected_snapshot)),
        )
        .expect("build save-as expectation");

        assert_eq!(
            expectation,
            SaveTargetExpectation::Unchanged(selected_snapshot)
        );
    }

    #[test]
    fn different_save_path_requires_selected_target_expectation() {
        let document_path = PathBuf::from("C:\\Temp\\original.txt");
        let save_path = PathBuf::from("C:\\Temp\\selected.txt");

        let error = save_target_expectation(
            Some(document_path.as_path()),
            save_path.as_path(),
            None,
            false,
            false,
            None,
        )
        .expect_err("different save path without selected expectation should fail");

        assert!(matches!(error, AppError::InvalidState(_)));
    }

    #[test]
    fn context_menu_screen_point_decodes_signed_coordinates() {
        let packed = ((-20i16 as u16 as u32) << 16) | u32::from(-10i16 as u16);

        let point = context_menu_screen_point(packed as LPARAM, null_mut())
            .expect("decode context menu point");

        assert_eq!(point.x, -10);
        assert_eq!(point.y, -20);
    }

    #[test]
    fn visible_whitespace_display_cache_reuses_rendered_text_for_same_generation() {
        let mut cache = HashMap::new();
        let key =
            VisibleWhitespaceDisplayCacheKey::new(DocumentId::new(3), 7, "a b\tc".len(), true);

        let first = visible_whitespace_display_text(&mut cache, key, "a b\tc");
        assert_eq!(first.as_str(), "a\u{00b7}b\u{2192}c");
        let first_rendered = match first {
            VisibleWhitespaceDisplayText::Rendered(rendered) => rendered,
            VisibleWhitespaceDisplayText::Source(_) => panic!("expected rendered text"),
        };

        let second = visible_whitespace_display_text(&mut cache, key, "a b\tc");
        assert_eq!(second.as_str(), "a\u{00b7}b\u{2192}c");
        let second_rendered = match second {
            VisibleWhitespaceDisplayText::Rendered(rendered) => rendered,
            VisibleWhitespaceDisplayText::Source(_) => panic!("expected cached rendered text"),
        };

        assert!(std::sync::Arc::ptr_eq(&first_rendered, &second_rendered));
    }

    #[test]
    fn visible_whitespace_display_cache_keeps_rendered_text_per_document() {
        let mut cache = HashMap::new();
        let first_key =
            VisibleWhitespaceDisplayCacheKey::new(DocumentId::new(3), 7, "a b".len(), true);
        let second_key =
            VisibleWhitespaceDisplayCacheKey::new(DocumentId::new(4), 1, "x\ty".len(), true);

        let first = visible_whitespace_display_text(&mut cache, first_key, "a b");
        let first_rendered = match first {
            VisibleWhitespaceDisplayText::Rendered(rendered) => rendered,
            VisibleWhitespaceDisplayText::Source(_) => panic!("expected rendered text"),
        };
        let second = visible_whitespace_display_text(&mut cache, second_key, "x\ty");
        assert_eq!(second.as_str(), "x\u{2192}y");

        let first_again = visible_whitespace_display_text(&mut cache, first_key, "a b");
        let first_again_rendered = match first_again {
            VisibleWhitespaceDisplayText::Rendered(rendered) => rendered,
            VisibleWhitespaceDisplayText::Source(_) => panic!("expected cached rendered text"),
        };

        assert!(std::sync::Arc::ptr_eq(
            &first_rendered,
            &first_again_rendered
        ));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn visible_whitespace_display_cache_invalidates_for_changed_generation() {
        let mut cache = HashMap::new();
        let first_key =
            VisibleWhitespaceDisplayCacheKey::new(DocumentId::new(3), 7, "a b".len(), true);
        let second_key =
            VisibleWhitespaceDisplayCacheKey::new(DocumentId::new(3), 8, "x y".len(), true);

        let first = visible_whitespace_display_text(&mut cache, first_key, "a b");
        let first_rendered = match first {
            VisibleWhitespaceDisplayText::Rendered(rendered) => rendered,
            VisibleWhitespaceDisplayText::Source(_) => panic!("expected rendered text"),
        };

        let second = visible_whitespace_display_text(&mut cache, second_key, "x y");
        assert_eq!(second.as_str(), "x\u{00b7}y");
        let second_rendered = match second {
            VisibleWhitespaceDisplayText::Rendered(rendered) => rendered,
            VisibleWhitespaceDisplayText::Source(_) => panic!("expected refreshed rendered text"),
        };

        assert!(!std::sync::Arc::ptr_eq(&first_rendered, &second_rendered));
        assert!(
            cache
                .get(&DocumentId::new(3))
                .is_some_and(|cached| cached.matches(second_key))
        );
    }

    #[test]
    fn visible_whitespace_display_cache_records_borrowed_display_without_string() {
        let mut cache = HashMap::new();
        let key = VisibleWhitespaceDisplayCacheKey::new(DocumentId::new(3), 7, "abc".len(), true);

        let display = visible_whitespace_display_text(&mut cache, key, "abc");

        assert_eq!(display.as_str(), "abc");
        assert!(matches!(display, VisibleWhitespaceDisplayText::Source(_)));
        assert!(
            cache
                .get(&DocumentId::new(3))
                .is_some_and(|cached| cached.matches(key) && cached.rendered.is_none())
        );
    }

    #[test]
    fn visible_whitespace_display_cache_clears_for_plain_text_mode() {
        let mut cache = HashMap::new();
        let whitespace_key =
            VisibleWhitespaceDisplayCacheKey::new(DocumentId::new(3), 7, "a b".len(), true);
        let plain_key =
            VisibleWhitespaceDisplayCacheKey::new(DocumentId::new(3), 7, "a b".len(), false);

        let _ = visible_whitespace_display_text(&mut cache, whitespace_key, "a b");
        assert!(!cache.is_empty());

        let display = visible_whitespace_display_text(&mut cache, plain_key, "a b");

        assert_eq!(display.as_str(), "a b");
        assert!(matches!(display, VisibleWhitespaceDisplayText::Source(_)));
        assert!(cache.is_empty());
    }

    #[test]
    fn pending_persistence_coalesces_requests_until_debounce_expires() {
        let mut pending = PendingPersistence::default();

        pending.request_recent_files();
        assert!(!pending.tick_elapsed());
        pending.request_settings();
        assert!(!pending.tick_elapsed());
        assert!(pending.tick_elapsed());

        let flushed = pending.take_pending();
        assert!(flushed.recent_files);
        assert!(flushed.settings);
        assert!(!pending.has_pending());
    }

    #[test]
    fn size_move_state_returns_pending_flags_and_resets_on_exit() {
        let mut state = SizeMoveDpiState::default();
        state.enter();
        state.mark_dpi_changed();
        state.defer_status_timer();

        let exit = state.exit();

        assert!(exit.dpi_changed);
        assert!(exit.status_timer_pending);
        assert_eq!(state, SizeMoveDpiState::default());
    }

    #[test]
    fn status_timer_action_skips_work_while_modal_dialog_is_active() {
        assert_eq!(
            status_timer_action(SizeMoveDpiState::default(), false),
            StatusTimerAction::Run
        );

        assert_eq!(
            status_timer_action(SizeMoveDpiState::default(), true),
            StatusTimerAction::SkipDuringModalDialog
        );

        let mut size_move = SizeMoveDpiState::default();
        size_move.enter();
        assert_eq!(
            status_timer_action(size_move, true),
            StatusTimerAction::DeferUntilSizeMoveExit
        );
    }

    #[test]
    fn about_dialog_text_includes_version_and_profile_link() {
        assert_eq!(ABOUT_DIALOG_TITLE, "About j3Text");
        assert_eq!(
            ABOUT_DIALOG_VERSION_LABEL,
            concat!("j3Text ", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(ABOUT_DIALOG_URL, "https://github.com/edgarp9");
        assert_eq!(ABOUT_DIALOG_WIDTH, 450);
        assert_eq!(ABOUT_DIALOG_HEIGHT, 400);
        assert_eq!(ABOUT_BODY_SCROLL_WIDTH, 402);
        assert_eq!(ABOUT_BODY_SCROLL_HEIGHT, 250);
        assert_eq!(ABOUT_BUTTON_TOP, 316);
        let body_text = about_dialog_body_text();
        assert_eq!(body_text.as_ref(), crate::notices::about_text().as_ref());
        assert!(!body_text.contains("## Resolved Rust Crates"));
        assert_eq!(ABOUT_DIALOG_CLASS_NAME, "J3TextAboutDialog");
        assert!(about_dialog_should_preprocess_message(WM_KEYDOWN));
        assert!(about_dialog_should_preprocess_message(WM_SYSKEYDOWN));
        assert!(!about_dialog_should_preprocess_message(WM_LBUTTONUP));
        assert!(!about_dialog_should_preprocess_message(WM_MOUSEWHEEL));
        assert_ne!(ABOUT_DIALOG_STYLE & WS_POPUP, 0);
        assert_eq!(ABOUT_DIALOG_STYLE & WS_CHILD, 0);
    }

    #[test]
    fn editor_chrome_snapshot_detects_status_and_ui_changes() {
        let status = CurrentEditorStatus {
            document_id: Some(DocumentId::new(1)),
            title: "note.txt".to_string(),
            path: None,
            is_dirty: false,
            can_save: true,
            can_save_as: true,
            can_edit: true,
            is_read_only: false,
            effective_read_only: false,
            word_wrap: false,
            can_undo: false,
            can_redo: false,
            line: 1,
            column: 1,
            selection_start_utf16: 0,
            selection_end_utf16: 0,
            char_count: 4,
            encoding: TextEncoding::Utf8,
            line_ending: LineEnding::Crlf,
            status_kind: crate::app::CurrentEditorStatusKind::Saved,
        };
        let settings = EditorSettings::default();
        let candidate = EditorChromeSnapshotCandidate {
            status: &status,
            settings: &settings,
            show_line_numbers: true,
            show_command_palette: false,
            editor_surface_present: true,
            dark_theme: false,
        };
        let snapshot = EditorChromeSnapshot::from_candidate(&candidate);

        assert!(snapshot.matches(&candidate));

        let moved_status = CurrentEditorStatus {
            line: 2,
            ..status.clone()
        };
        let moved_candidate = EditorChromeSnapshotCandidate {
            status: &moved_status,
            ..candidate
        };
        assert!(!snapshot.matches(&moved_candidate));

        let mut changed_settings = settings.clone();
        changed_settings.word_wrap = !changed_settings.word_wrap;
        let settings_candidate = EditorChromeSnapshotCandidate {
            settings: &changed_settings,
            ..candidate
        };
        assert!(!snapshot.matches(&settings_candidate));

        let surface_candidate = EditorChromeSnapshotCandidate {
            editor_surface_present: false,
            ..candidate
        };
        assert!(!snapshot.matches(&surface_candidate));
    }

    #[test]
    fn dpi_metrics_scales_pixels_from_window_dpi() {
        let metrics = DpiMetrics::new(144);

        assert_eq!(metrics.ui_scale().px(10), 15);
        assert_eq!(DpiMetrics::new(0), DpiMetrics::default());
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
    fn startup_file_paths_skip_executable_and_empty_args() {
        let paths = startup_file_paths_from_args([
            OsString::from("j3text.exe"),
            OsString::from(""),
            OsString::from("note.txt"),
            OsString::from("C:\\Temp\\second.txt"),
        ]);

        assert_eq!(
            paths,
            vec![
                PathBuf::from("note.txt"),
                PathBuf::from("C:\\Temp\\second.txt")
            ]
        );
    }

    #[test]
    fn main_window_create_failure_uses_captured_inner_error() {
        const ERROR_CANNOT_FIND_WND_CLASS_LOCAL: u32 = 1407;
        let mut context = MainWindowCreateContext {
            window_ptr: null_mut(),
            owned_by_window: true,
            create_error: Some(AppError::win32(
                "create Rich Edit text surface",
                ERROR_CANNOT_FIND_WND_CLASS_LOCAL,
            )),
        };

        let error = main_window_create_failure_error(&mut context);

        assert_eq!(
            error.to_string(),
            "create Rich Edit text surface: Win32 error 1407"
        );
        assert!(context.create_error.is_none());
    }

    #[test]
    fn main_window_create_failure_falls_back_to_last_win32_error() {
        const ERROR_INVALID_WINDOW_HANDLE_LOCAL: u32 = 1400;
        let mut context = MainWindowCreateContext {
            window_ptr: null_mut(),
            owned_by_window: false,
            create_error: None,
        };
        unsafe {
            SetLastError(ERROR_INVALID_WINDOW_HANDLE_LOCAL);
        }

        let error = main_window_create_failure_error(&mut context);

        assert_eq!(error.to_string(), "create main window: Win32 error 1400");
    }

    #[test]
    fn listbox_add_string_result_treats_errspace_as_failure() {
        assert!(!listbox_add_string_failed(0));
        assert!(!listbox_add_string_failed(3));
        assert!(listbox_add_string_failed(LB_ERR));
        assert!(listbox_add_string_failed(LB_ERRSPACE));
    }

    #[test]
    fn combo_add_string_result_treats_errspace_as_failure() {
        assert!(!combo_add_string_failed(0));
        assert!(!combo_add_string_failed(3));
        assert!(combo_add_string_failed(CB_ERR));
        assert!(combo_add_string_failed(CB_ERRSPACE));
    }

    #[test]
    fn command_palette_id_is_recorded_only_after_listbox_add_success() {
        let mut ids = Vec::new();

        let success =
            record_command_palette_id_after_listbox_add(&mut ids, EditorCommandId::NewFile, Ok(()));

        assert!(success.is_ok());
        assert_eq!(ids.len(), 1);
        assert!(matches!(
            ids.first().copied(),
            Some(EditorCommandId::NewFile)
        ));

        let failure = record_command_palette_id_after_listbox_add(
            &mut ids,
            EditorCommandId::OpenFile,
            Err(AppError::InvalidState("add listbox item failed")),
        );

        assert!(failure.is_err());
        assert_eq!(ids.len(), 1);
        assert!(matches!(
            ids.first().copied(),
            Some(EditorCommandId::NewFile)
        ));
    }

    #[test]
    fn modal_dialog_guard_tracks_nested_active_dialog_state() {
        assert!(!modal_dialog_active());

        let first = ActiveModalDialogGuard::enter();
        assert!(modal_dialog_active());

        {
            let second = ActiveModalDialogGuard::enter();
            assert!(modal_dialog_active());
            drop(second);
            assert!(modal_dialog_active());
        }

        drop(first);
        assert!(!modal_dialog_active());
    }

    #[test]
    fn custom_modal_dialog_guard_also_marks_modal_dialog_active() {
        assert!(!modal_dialog_active());

        let owner = CustomModalDialogGuard::install(null_mut());
        assert!(modal_dialog_active());

        drop(owner);
        assert!(!modal_dialog_active());
    }

    #[test]
    fn create_child_missing_class_preserves_create_window_error() -> Result<(), AppError> {
        const ERROR_CANNOT_FIND_WND_CLASS_LOCAL: u32 = 1407;
        register_test_parent_window_class()?;
        let parent = create_test_parent_window()?;
        let mut window = main_window_without_store();
        window.hwnd = parent;
        unsafe {
            SetLastError(ERROR_SUCCESS);
        }

        let error = match window.create_child("J3TextMissingChildClass", "", WS_TABSTOP, 0, 0) {
            Ok(child) => {
                destroy_test_window(child);
                destroy_test_window(parent);
                return Err(AppError::InvalidState(
                    "missing child window class unexpectedly created a control",
                ));
            }
            Err(error) => error,
        };
        destroy_test_window(parent);

        match error {
            AppError::Win32 { code, context, .. } => {
                assert_eq!(context, "create child control");
                assert_eq!(code, ERROR_CANNOT_FIND_WND_CLASS_LOCAL);
            }
            _ => {
                return Err(AppError::InvalidState(
                    "missing child window class did not return a Win32 error",
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn edit_line_scroll_delta_moves_in_both_directions() {
        assert_eq!(edit_line_scroll_delta(3, 8), 5);
        assert_eq!(edit_line_scroll_delta(8, 3), -5);
        assert_eq!(edit_line_scroll_delta(4, 4), 0);
    }

    #[test]
    fn rich_edit_offset_mapping_treats_crlf_as_one_control_unit() {
        let text = "a\r\nb";

        assert_eq!(text_offset_to_rich_edit_offset(text, 0), 0);
        assert_eq!(text_offset_to_rich_edit_offset(text, 1), 1);
        assert_eq!(text_offset_to_rich_edit_offset(text, 3), 2);
        assert_eq!(text_offset_to_rich_edit_offset(text, 4), 3);

        assert_eq!(rich_edit_offset_to_text_offset(text, 0), 0);
        assert_eq!(rich_edit_offset_to_text_offset(text, 1), 1);
        assert_eq!(rich_edit_offset_to_text_offset(text, 2), 3);
        assert_eq!(rich_edit_offset_to_text_offset(text, 3), 4);
    }

    #[test]
    fn rich_edit_offset_mapping_preserves_lf_cr_and_surrogate_widths() {
        let text = "a\n😀\rc";

        assert_eq!(text_offset_to_rich_edit_offset(text, 2), 2);
        assert_eq!(text_offset_to_rich_edit_offset(text, 4), 4);
        assert_eq!(rich_edit_offset_to_text_offset(text, 4), 4);
        assert_eq!(rich_edit_offset_to_text_offset(text, 6), 6);
    }

    #[test]
    fn search_offset_mapping_keeps_crlf_and_surrogate_selection_boundaries() {
        let text = "a\r\nb😀c";

        assert_eq!(rich_edit_offset_to_byte_index(text, 2), 3);
        assert_eq!(rich_edit_offset_to_byte_index(text, 4), 4);
        assert_eq!(rich_edit_offset_to_byte_index(text, 5), 8);

        assert_eq!(byte_range_to_rich_edit_offsets(text, 3, 8), (2, 5));
        assert_eq!(byte_range_to_rich_edit_offsets("a\r\nb", 2, 3), (2, 2));
    }

    #[test]
    fn find_next_previous_offset_mapping_reuses_long_prefix_checkpoints() {
        let prefix = "a\r\n한😀\n".repeat(SEARCH_OFFSET_CHECKPOINT_RICH_UNITS / 2);
        let text = format!("first\r\n{prefix}middle😀\r\nlast");
        let start_text_units = "first\r\n".encode_utf16().count() + prefix.encode_utf16().count();
        let rich_start = text_offset_to_rich_edit_offset(&text, start_text_units);
        let content: Arc<str> = Arc::from(text.as_str());
        let document_id = DocumentId::new(1);
        let mut cache = None;

        assert_eq!(
            find_text_rich_edit_offsets_cached(
                &mut cache,
                document_id,
                Arc::clone(&content),
                "last",
                rich_start,
                SearchDirection::Forward,
            ),
            expected_search_selection(&text, "last", rich_start, SearchDirection::Forward)
        );
        let checkpoints_after_forward = cache
            .as_ref()
            .map_or(0, |cache| cache.prefix_checkpoints.len());
        assert!(checkpoints_after_forward > 1);
        assert_eq!(
            find_text_rich_edit_offsets_cached(
                &mut cache,
                document_id,
                Arc::clone(&content),
                "first",
                rich_start,
                SearchDirection::Backward,
            ),
            expected_search_selection(&text, "first", rich_start, SearchDirection::Backward)
        );
        assert_eq!(
            cache
                .as_ref()
                .map_or(0, |cache| cache.prefix_checkpoints.len()),
            checkpoints_after_forward
        );
    }

    #[test]
    fn search_result_selection_mapping_reuses_long_prefix_checkpoints() {
        let prefix = "a\r\n한😀\n".repeat(SEARCH_OFFSET_CHECKPOINT_RICH_UNITS / 2);
        let text = format!("{prefix}NEEDLE\r\n한😀\n{prefix}NEEDLE");
        let content: Arc<str> = Arc::from(text.as_str());
        let document_id = DocumentId::new(1);
        let results = crate::domain::collect_search_results(&text, "NEEDLE", 10);
        let mut cache = None;

        assert_eq!(results.len(), 2);
        let first_selection = search_result_rich_edit_offsets_cached(
            &mut cache,
            document_id,
            Arc::clone(&content),
            results[0].range.start,
            results[0].range.end,
        );
        assert_eq!(
            first_selection,
            text_offset_range_to_rich_edit_offsets(
                &text,
                results[0].utf16_range.start,
                results[0].utf16_range.end,
            )
        );
        let checkpoints_after_first = cache
            .as_ref()
            .map_or(0, |cache| cache.prefix_checkpoints.len());
        assert!(checkpoints_after_first > 1);
        let checkpoint_for_second = nearest_search_prefix_checkpoint_by_byte(
            &cache
                .as_ref()
                .expect("search result selection should populate offset cache")
                .prefix_checkpoints,
            results[1].range.start,
        );
        assert!(checkpoint_for_second.byte_index > 0);

        let second_selection = search_result_rich_edit_offsets_cached(
            &mut cache,
            document_id,
            Arc::clone(&content),
            results[1].range.start,
            results[1].range.end,
        );
        assert_eq!(
            second_selection,
            text_offset_range_to_rich_edit_offsets(
                &text,
                results[1].utf16_range.start,
                results[1].utf16_range.end,
            )
        );
        assert!(
            cache
                .as_ref()
                .is_some_and(|cache| cache.prefix_checkpoints.len() >= checkpoints_after_first)
        );
    }

    #[test]
    fn search_offset_cache_resets_for_same_length_content_change() {
        let document_id = DocumentId::new(1);
        let prefix_rich_units = SEARCH_OFFSET_CHECKPOINT_RICH_UNITS * 2;
        let first_text = format!("{}needle", "a".repeat(prefix_rich_units));
        let first_content: Arc<str> = Arc::from(first_text.as_str());
        let mut cache = None;

        assert_eq!(
            find_text_rich_edit_offsets_cached(
                &mut cache,
                document_id,
                Arc::clone(&first_content),
                "needle",
                prefix_rich_units,
                SearchDirection::Forward,
            ),
            expected_search_selection(
                &first_text,
                "needle",
                prefix_rich_units,
                SearchDirection::Forward,
            )
        );
        assert!(
            cache
                .as_ref()
                .is_some_and(|cache| cache.prefix_checkpoints.len() > 1)
        );

        let second_text = format!("{}needle", "\r\n".repeat(prefix_rich_units / 2));
        assert_eq!(second_text.len(), first_text.len());
        let second_content: Arc<str> = Arc::from(second_text.as_str());
        let second_rich_start = prefix_rich_units / 2;

        assert_eq!(
            find_text_rich_edit_offsets_cached(
                &mut cache,
                document_id,
                Arc::clone(&second_content),
                "needle",
                second_rich_start,
                SearchDirection::Forward,
            ),
            expected_search_selection(
                &second_text,
                "needle",
                second_rich_start,
                SearchDirection::Forward,
            )
        );
        assert!(
            cache
                .as_ref()
                .is_some_and(|cache| Arc::ptr_eq(&cache.content, &second_content))
        );
    }

    #[test]
    fn range_offset_mapping_matches_single_offset_mapping_for_mixed_prefix() -> Result<(), AppError>
    {
        let prefix = "a\r\n한😀\n".repeat(256);
        let text = format!("{prefix}mid\r\n😀끝");
        let start_units = prefix.encode_utf16().count();
        let end_units = start_units + "mid\r\n😀".encode_utf16().count();

        let expected_start = text_offset_to_rich_edit_offset(&text, start_units);
        let expected_end = text_offset_to_rich_edit_offset(&text, end_units);
        assert_eq!(
            text_offset_range_to_rich_edit_offsets(&text, start_units, end_units),
            (expected_start, expected_end)
        );
        assert_eq!(
            text_offset_range_to_rich_edit_offsets(&text, end_units, start_units),
            (expected_end, expected_start)
        );
        assert_eq!(
            text_offset_range_to_rich_edit_offsets("a\r\nb", 2, 3),
            (2, 2)
        );

        let start_byte = rich_edit_offset_to_byte_index(&text, expected_start);
        let end_byte = rich_edit_offset_to_byte_index(&text, expected_end);
        assert_eq!(
            rich_edit_offset_range_to_byte_indices(&text, expected_start, expected_end),
            (start_byte, end_byte)
        );
        assert_eq!(
            rich_edit_offset_range_to_byte_indices(&text, expected_end, expected_start),
            (end_byte, start_byte)
        );
        assert_eq!(
            rich_edit_selection_byte_range(&text, expected_end, expected_start)?,
            (start_byte, end_byte)
        );
        Ok(())
    }

    fn expected_search_selection(
        text: &str,
        query: &str,
        rich_start: usize,
        direction: SearchDirection,
    ) -> Option<(usize, usize)> {
        let start_byte = rich_edit_offset_to_byte_index(text, rich_start);
        find_text(text, query, start_byte, direction)
            .map(|range| byte_range_to_rich_edit_offsets(text, range.start, range.end))
    }

    #[test]
    fn rich_edit_input_limits_follow_document_load_policy() {
        assert_eq!(
            RICH_EDIT_EDITABLE_TEXT_LIMIT as u64,
            MAX_DOCUMENT_LOAD_BYTES
        );
        assert_eq!(
            RICH_EDIT_SURFACE_TEXT_LIMIT as u64,
            MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES
        );
        const { assert!(RICH_EDIT_EDITABLE_TEXT_LIMIT <= RICH_EDIT_SURFACE_TEXT_LIMIT) };
    }

    #[test]
    fn rich_edit_surface_length_rejects_oversized_read_before_allocation() {
        assert!(validate_rich_edit_text_units(RICH_EDIT_SURFACE_TEXT_LIMIT_UNITS).is_ok());
        assert!(matches!(
            validate_rich_edit_text_units(RICH_EDIT_SURFACE_TEXT_LIMIT_UNITS + 1),
            Err(AppError::InvalidState(
                RICH_EDIT_SURFACE_TEXT_TOO_LARGE_MESSAGE
            ))
        ));
    }

    #[test]
    fn editor_document_byte_limit_rejects_oversized_sync() {
        assert!(validate_editor_document_byte_len(MAX_DOCUMENT_LOAD_BYTES).is_ok());
        assert!(matches!(
            validate_editor_document_byte_len(MAX_DOCUMENT_LOAD_BYTES + 1),
            Err(AppError::InvalidState(
                EDITOR_DOCUMENT_TEXT_TOO_LARGE_MESSAGE
            ))
        ));
    }

    #[test]
    fn rich_edit_utf8_byte_limit_rejects_oversized_sync() {
        assert!(
            validate_rich_edit_utf8_byte_len(
                MAX_DOCUMENT_LOAD_BYTES,
                Some(MAX_DOCUMENT_LOAD_BYTES)
            )
            .is_ok()
        );
        assert!(matches!(
            validate_rich_edit_utf8_byte_len(
                MAX_DOCUMENT_LOAD_BYTES + 1,
                Some(MAX_DOCUMENT_LOAD_BYTES)
            ),
            Err(AppError::InvalidState(
                EDITOR_DOCUMENT_TEXT_TOO_LARGE_MESSAGE
            ))
        ));
    }

    #[test]
    fn replace_all_text_matches_standard_replace() -> Result<(), AppError> {
        let cases = [
            ("alpha alpha", "alpha", "beta gamma"),
            ("한글 한글", "한", "han"),
            ("aaaa", "a", "bb"),
            ("aaaa", "aa", "b"),
            ("same", "same", "same"),
            ("abc", "z", "long"),
        ];

        for (text, query, replacement) in cases {
            let expected = text.replace(query, replacement);
            let actual =
                replace_all_text_if_changed(text, text.chars().count(), query, replacement)?;

            match actual {
                Some(actual) => {
                    assert_ne!(expected, text);
                    assert_eq!(actual.text, expected);
                    assert_eq!(
                        actual.byte_len,
                        editor_document_byte_len_from_usize(actual.text.len())?
                    );
                    assert_eq!(
                        actual.metrics,
                        DocumentMetrics::from_char_count(actual.text.chars().count())
                    );
                }
                None => assert_eq!(expected, text),
            }
        }

        Ok(())
    }

    #[test]
    fn replace_all_text_capacity_tracks_smaller_result() -> Result<(), AppError> {
        let actual = replace_all_text_if_changed(
            "delete delete delete",
            "delete delete delete".chars().count(),
            "delete",
            "",
        )?
        .ok_or(AppError::InvalidState(
            "test Replace All should change text",
        ))?;

        assert_eq!(actual.text, "  ");
        assert_eq!(actual.byte_len, 2);
        assert_eq!(actual.text.capacity(), actual.text.len());
        Ok(())
    }

    #[test]
    fn replace_all_result_byte_len_uses_checked_arithmetic() {
        let oversized = checked_replace_all_result_byte_len_add(MAX_DOCUMENT_LOAD_BYTES, 1)
            .expect("calculate oversized replace all byte length");
        assert!(matches!(
            validate_editor_document_byte_len(oversized),
            Err(AppError::InvalidState(
                EDITOR_DOCUMENT_TEXT_TOO_LARGE_MESSAGE
            ))
        ));

        assert!(matches!(
            checked_replace_all_result_byte_len_add(u64::MAX, 1),
            Err(AppError::InvalidState(
                REPLACE_ALL_RESULT_TEXT_LENGTH_OVERFLOW_MESSAGE
            ))
        ));
    }

    #[test]
    fn replace_all_text_rejects_result_limit_before_appending_part() {
        let error = match replace_all_text_if_changed_with_policy(
            "a a",
            "a a".chars().count(),
            "a",
            "xx",
            |byte_len| byte_len <= 3,
        ) {
            Ok(_) => panic!("oversized replace all should fail"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            AppError::InvalidState(EDITOR_DOCUMENT_TEXT_TOO_LARGE_MESSAGE)
        ));
    }

    #[test]
    fn selection_replacement_result_byte_len_matches_standard_replace() -> Result<(), AppError> {
        let text = "a\r\nb😀c";
        let (start_byte, end_byte) = rich_edit_selection_byte_range(text, 2, 5)?;
        let replacement = "xyz";
        let selected = text
            .get(start_byte..end_byte)
            .ok_or(AppError::InvalidState(
                "test selection should align to text boundaries",
            ))?;

        assert_eq!(selected, "b😀");
        assert_eq!(
            checked_selection_replacement_result_byte_len(
                text.len() as u64,
                selected.len() as u64,
                replacement.len() as u64,
            )?,
            (text.len() - selected.len() + replacement.len()) as u64
        );
        validate_selection_replacement_document_size(text, start_byte, end_byte, replacement)
    }

    #[test]
    fn selection_replacement_result_byte_len_uses_checked_arithmetic() {
        let oversized =
            checked_selection_replacement_result_byte_len(MAX_DOCUMENT_LOAD_BYTES, 0, 1)
                .expect("calculate oversized replacement byte length");
        assert!(matches!(
            validate_editor_document_byte_len(oversized),
            Err(AppError::InvalidState(
                EDITOR_DOCUMENT_TEXT_TOO_LARGE_MESSAGE
            ))
        ));

        assert!(matches!(
            checked_selection_replacement_result_byte_len(0, 1, 0),
            Err(AppError::InvalidState(
                REPLACE_ALL_RESULT_TEXT_LENGTH_OVERFLOW_MESSAGE
            ))
        ));
    }

    #[test]
    fn selected_char_count_cache_accepts_rich_edit_offsets() {
        let text = "a\r\nb😀c";
        let start = text_offset_to_rich_edit_offset(text, 3);
        let end = text_offset_to_rich_edit_offset(text, 6);
        let mut cache = None;

        let selected = selected_char_count_cached(
            &mut cache,
            DocumentId::new(1),
            text,
            start as u32,
            end as u32,
        );

        assert_eq!(selected, 2);
        assert_eq!(
            selected_char_count_cached(
                &mut cache,
                DocumentId::new(1),
                text,
                start as u32,
                end as u32,
            ),
            2
        );
        assert_eq!(
            selected_char_count_cached(
                &mut cache,
                DocumentId::new(1),
                text,
                end as u32,
                start as u32,
            ),
            2
        );
    }

    #[test]
    fn selected_char_count_from_rich_edit_offsets_matches_existing_mapping_cases() {
        let cases = [
            ("a\r\nb", 1, 2),
            ("a\nb\rc", 1, 4),
            ("a한b", 1, 2),
            ("a😀b", 1, 3),
            ("a😀b", 1, 2),
            ("a😀b", 2, 3),
        ];

        for (text, start, end) in cases {
            let expected = selected_char_count_via_existing_offset_mapping(text, start, end);

            assert_eq!(
                selected_char_count_from_rich_edit_offsets(text, start, end),
                expected,
                "{text:?} {start}..{end}"
            );
            assert_eq!(
                selected_char_count_from_rich_edit_offsets(text, end, start),
                expected,
                "{text:?} {end}..{start}"
            );
        }
    }

    #[test]
    fn selected_char_count_cache_skips_document_scan_for_caret() {
        let text = "a\r\n".repeat(1024);
        let mut cache = None;

        let selected =
            selected_char_count_cached(&mut cache, DocumentId::new(1), &text, 2048, 2048);

        assert_eq!(selected, 0);
        assert_eq!(
            cache
                .as_ref()
                .map_or(0, |cache| cache.prefix_checkpoints.len()),
            1
        );
    }

    #[test]
    fn selected_char_count_cache_keeps_prefix_checkpoints_for_changed_selection() {
        let prefix_len = SELECTION_METRICS_CHECKPOINT_RICH_UNITS * 3;
        let text = format!("{}😀z", "a".repeat(prefix_len));
        let mut cache = None;

        let selected =
            selected_char_count_cached(&mut cache, DocumentId::new(1), &text, 0, prefix_len as u32);

        assert_eq!(selected, prefix_len);
        let checkpoints_after_first = cache
            .as_ref()
            .map_or(0, |cache| cache.prefix_checkpoints.len());
        assert!(checkpoints_after_first > 1);

        let selected = selected_char_count_cached(
            &mut cache,
            DocumentId::new(1),
            &text,
            (SELECTION_METRICS_CHECKPOINT_RICH_UNITS * 2) as u32,
            (prefix_len + 2) as u32,
        );

        assert_eq!(selected, SELECTION_METRICS_CHECKPOINT_RICH_UNITS + 1);
        assert!(
            cache
                .as_ref()
                .is_some_and(|cache| cache.prefix_checkpoints.len() >= checkpoints_after_first)
        );
    }

    #[test]
    fn selected_char_count_cache_reuses_recent_prefix_metrics_between_selections() {
        let selection_start = SELECTION_METRICS_CHECKPOINT_RICH_UNITS + 37;
        let first_selection_end = selection_start + 13;
        let second_selection_end = first_selection_end + 11;
        let text = "a".repeat(second_selection_end + 1);
        let mut cache = None;

        let selected = selected_char_count_cached(
            &mut cache,
            DocumentId::new(1),
            &text,
            selection_start as u32,
            first_selection_end as u32,
        );

        assert_eq!(selected, 13);
        let Some(cache_ref) = cache.as_ref() else {
            panic!("selection metrics cache should be initialized");
        };
        assert_eq!(
            cache_ref
                .nearest_prefix_checkpoint(second_selection_end)
                .rich_offset,
            first_selection_end
        );

        let selected = selected_char_count_cached(
            &mut cache,
            DocumentId::new(1),
            &text,
            selection_start as u32,
            second_selection_end as u32,
        );

        assert_eq!(selected, 24);
    }

    fn selected_char_count_via_existing_offset_mapping(
        text: &str,
        rich_selection_start: usize,
        rich_selection_end: usize,
    ) -> usize {
        let selection_start = rich_edit_offset_to_text_offset(text, rich_selection_start);
        let selection_end = rich_edit_offset_to_text_offset(text, rich_selection_end);
        let start = selection_start.min(selection_end);
        let end = selection_start.max(selection_end);
        let start_byte = utf16_offset_to_byte_index(text, start);
        let end_byte = utf16_offset_to_byte_index(text, end);

        text.get(start_byte..end_byte)
            .map_or(0, |selected| selected.chars().count())
    }

    #[test]
    #[ignore = "loads a multi-MB Rich Edit control for local UI surface timing"]
    fn measure_rich_edit_large_text_surface_sync() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let surface = fixture.surface();
        let text = repeated_large_editor_text(2 * 1024 * 1024);

        let started = std::time::Instant::now();
        surface.set_text(&text)?;
        eprintln!("rich_edit_set_text: {:?}", started.elapsed());

        let started = std::time::Instant::now();
        let copied = surface.get_text()?;
        eprintln!(
            "rich_edit_get_text: {:?}, chars={}",
            started.elapsed(),
            copied.chars().count()
        );
        assert_eq!(copied, text);
        Ok(())
    }

    fn repeated_large_editor_text(target_bytes: usize) -> String {
        let mut text = String::with_capacity(target_bytes);
        let lines = [
            "alpha NEEDLE 한글 日本語\r\n",
            "plain line without match\r\n",
            "carriage return NEEDLE line\r\n",
            "long segment xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\r\n",
        ];
        while text.len() < target_bytes {
            for line in lines {
                if text.len() >= target_bytes {
                    break;
                }
                text.push_str(line);
            }
        }
        text
    }

    struct RichEditFixture {
        _guard: std::sync::MutexGuard<'static, ()>,
        parent: HWND,
        edit: HWND,
    }

    impl RichEditFixture {
        fn new() -> Result<Self, AppError> {
            Self::new_with_plain_text_mode(true)
        }

        fn new_without_plain_text_mode() -> Result<Self, AppError> {
            Self::new_with_plain_text_mode(false)
        }

        fn new_with_plain_text_mode(enable_plain_text_mode: bool) -> Result<Self, AppError> {
            let guard = rich_edit_test_guard();
            init_rich_edit()?;
            if !rich_edit_module_is_loaded() {
                return Err(AppError::InvalidState(
                    "Rich Edit library was not loaded before creating the test control",
                ));
            }
            register_test_parent_window_class()?;
            let parent = create_test_parent_window()?;
            let edit = match create_test_rich_edit_control(parent) {
                Ok(edit) => edit,
                Err(error) => {
                    destroy_test_window(parent);
                    return Err(error);
                }
            };
            if enable_plain_text_mode {
                if let Err(error) = set_rich_edit_plain_text_mode(edit) {
                    destroy_test_window(edit);
                    destroy_test_window(parent);
                    return Err(error);
                }
                set_rich_edit_event_mask(edit);
                set_rich_edit_text_limit(edit, RICH_EDIT_EDITABLE_TEXT_LIMIT);
            }
            Ok(Self {
                _guard: guard,
                parent,
                edit,
            })
        }

        fn surface(&self) -> EditorTextSurface {
            EditorTextSurface::from_hwnd(self.edit).expect("test Rich Edit surface")
        }
    }

    impl Drop for RichEditFixture {
        fn drop(&mut self) {
            destroy_test_window(self.edit);
            destroy_test_window(self.parent);
        }
    }

    fn rich_edit_test_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        match LOCK.get_or_init(|| std::sync::Mutex::new(())).lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn rich_edit_module_is_loaded() -> bool {
        let module_name = wide_null("Msftedit.dll");
        !unsafe { GetModuleHandleW(module_name.as_ptr()) }.is_null()
    }

    fn destroy_test_window(hwnd: HWND) {
        if !hwnd.is_null() {
            unsafe {
                DestroyWindow(hwnd);
            }
        }
    }

    fn create_test_rich_edit_control(parent: HWND) -> Result<HWND, AppError> {
        let class_name = wide_null(RICH_EDIT_CLASS);
        let empty = wide_null("");
        let edit = unsafe {
            CreateWindowExW(
                WS_EX_CLIENTEDGE,
                class_name.as_ptr(),
                empty.as_ptr(),
                WS_CHILD | WS_VISIBLE | ES_MULTILINE | ES_AUTOVSCROLL | ES_NOHIDESEL | WS_VSCROLL,
                0,
                0,
                640,
                480,
                parent,
                null_mut(),
                GetModuleHandleW(null()),
                null_mut(),
            )
        };
        if edit.is_null() {
            return Err(last_win32_error("create hidden Rich Edit test control"));
        }
        Ok(edit)
    }

    fn register_test_parent_window_class() -> Result<(), AppError> {
        const ERROR_CLASS_ALREADY_EXISTS_LOCAL: u32 = 1410;
        static REGISTER: OnceLock<Result<(), u32>> = OnceLock::new();

        match REGISTER.get_or_init(|| {
            let class_name = wide_null("J3TextRichEditTestParent");
            let instance = unsafe { GetModuleHandleW(null()) };
            let wnd_class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(DefWindowProcW),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: null_mut(),
                hCursor: null_mut(),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: class_name.as_ptr(),
                hIconSm: null_mut(),
            };

            let atom = unsafe { RegisterClassExW(&wnd_class) };
            if atom == 0 {
                let error = unsafe { GetLastError() };
                if error == ERROR_CLASS_ALREADY_EXISTS_LOCAL {
                    Ok(())
                } else {
                    Err(error)
                }
            } else {
                Ok(())
            }
        }) {
            Ok(()) => Ok(()),
            Err(code) => Err(AppError::win32(
                "register Rich Edit test parent window class",
                *code,
            )),
        }
    }

    fn create_test_parent_window() -> Result<HWND, AppError> {
        let class_name = wide_null("J3TextRichEditTestParent");
        let title = wide_null("");
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                800,
                600,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null_mut(),
            )
        };
        if hwnd.is_null() {
            return Err(last_win32_error("create hidden Rich Edit parent window"));
        }
        Ok(hwnd)
    }

    #[test]
    fn rich_edit_initialization_loads_msftedit_before_control_creation() -> Result<(), AppError> {
        init_rich_edit()?;
        if !rich_edit_module_is_loaded() {
            return Err(AppError::InvalidState(
                "Msftedit.dll was not loaded before Rich Edit control creation",
            ));
        }
        let fixture = RichEditFixture::new()?;

        assert!(!fixture.edit.is_null());
        Ok(())
    }

    #[test]
    fn rich_edit_plain_text_mode_rejects_existing_text() -> Result<(), AppError> {
        let fixture = RichEditFixture::new_without_plain_text_mode()?;
        set_window_text(fixture.edit, "already loaded")?;

        let error = match set_rich_edit_plain_text_mode(fixture.edit) {
            Ok(()) => {
                return Err(AppError::InvalidState(
                    "Rich Edit accepted plain text mode after text was loaded",
                ));
            }
            Err(error) => error,
        };

        assert!(matches!(
            error,
            AppError::Platform {
                kind: PlatformErrorKind::RichEditPlainTextModeFailed,
                ..
            }
        ));
        assert!(
            error
                .to_string()
                .contains("must be set before loading document text")
        );
        Ok(())
    }

    #[test]
    fn rich_edit_plain_text_round_trip_keeps_literal_content() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let text = "한글 English 123 !@#$%^&*()[]{} \"'\\\r\nsecond line\r\n{\\rtf1\\b not rich}";
        let surface = fixture.surface();

        surface.set_text(text)?;

        assert_eq!(surface.get_text()?, text);
        Ok(())
    }

    #[test]
    fn rich_edit_plain_text_round_trip_keeps_empty_content() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let surface = fixture.surface();

        surface.set_text("")?;

        assert_eq!(surface.get_text()?, "");
        Ok(())
    }

    #[test]
    fn rich_edit_plain_text_round_trip_preserves_surrogate_pairs() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let text = "empty?\r\n한글 English 123 !@# 😀🚀𐐷\r\nlast line";
        let surface = fixture.surface();

        surface.set_text(text)?;

        assert_eq!(surface.get_text()?, text);
        Ok(())
    }

    #[test]
    fn rich_edit_document_sync_preserves_utf8_text_and_metrics() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let text = "alpha\r\n한글 English 123 !@# 😀🚀𐐷\r\nlast line";
        let surface = fixture.surface();

        surface.set_text(text)?;

        let actual = surface.get_text_for_document_sync()?;
        assert_eq!(actual.text, text);
        assert_eq!(
            actual.metrics,
            DocumentMetrics::from_char_count(text.chars().count())
        );
        Ok(())
    }

    #[test]
    fn rich_edit_plain_text_with_limit_rejects_utf8_byte_overflow() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let surface = fixture.surface();
        surface.set_text("한글")?;

        let error = match get_rich_edit_plain_text_with_limit(fixture.edit, Some(5)) {
            Ok(_) => {
                return Err(AppError::InvalidState(
                    "Rich Edit text exceeded the test byte limit",
                ));
            }
            Err(error) => error,
        };

        assert!(matches!(
            error,
            AppError::InvalidState(EDITOR_DOCUMENT_TEXT_TOO_LARGE_MESSAGE)
        ));
        Ok(())
    }

    #[test]
    fn rich_edit_plain_text_utf8_byte_length_matches_plain_text() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let text = "alpha\r\n한글 😀";
        let surface = fixture.surface();

        surface.set_text(text)?;

        assert_eq!(
            rich_edit_plain_text_utf8_byte_len(fixture.edit)?,
            text.len() as u64
        );
        Ok(())
    }

    #[test]
    fn decode_rich_edit_plain_text_preserves_utf8_width_text_and_metrics() -> Result<(), AppError> {
        let cases = ["ASCII only", "한글 문서", "emoji 😀 and 𐐷"];

        for text in cases {
            let units: Vec<u16> = text.encode_utf16().collect();
            let decoded = decode_rich_edit_plain_text(&units, None)?;

            assert_eq!(decoded.text, text);
            assert!(
                decoded.metrics == DocumentMetrics::from_char_count(text.chars().count()),
                "decoded metrics should match char count"
            );
            assert_eq!(
                rich_edit_utf8_capacity_from_utf16_units(&units),
                Some(text.len())
            );
        }

        Ok(())
    }

    #[test]
    fn rich_edit_selection_replace_is_plain_text_and_undoable() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let surface = fixture.surface();
        surface.set_text("alpha beta")?;
        surface.set_selection(6, 10);

        surface.replace_selection("한글 {\\rtf1}")?;

        assert_eq!(surface.get_text()?, "alpha 한글 {\\rtf1}");
        assert!(surface.can_undo());
        surface.undo();
        assert_eq!(surface.get_text()?, "alpha beta");
        assert!(surface.can_redo());
        surface.redo();
        assert_eq!(surface.get_text()?, "alpha 한글 {\\rtf1}");
        Ok(())
    }

    #[test]
    fn rich_edit_select_all_replacement_preserves_plain_unicode() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let surface = fixture.surface();
        surface.set_text("old\r\ntext")?;
        surface.select_all();

        surface.replace_selection("새 text 123 😀")?;

        assert_eq!(surface.get_text()?, "새 text 123 😀");
        Ok(())
    }

    #[test]
    fn rich_edit_word_wrap_toggle_does_not_mutate_plain_text() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let surface = fixture.surface();
        let text = "long line that wraps only in view\r\nsecond line";

        surface.set_text(text)?;
        surface.set_word_wrap(true);
        assert_eq!(surface.get_text()?, text);
        surface.set_word_wrap(false);
        assert_eq!(surface.get_text()?, text);
        Ok(())
    }

    #[test]
    fn rich_edit_line_and_column_can_reuse_selection_start() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let surface = fixture.surface();
        surface.set_text("first\r\nsecond")?;
        surface.set_selection(0, 0);

        assert_eq!(surface.line_and_column_from_selection_start(7), (2, 2));
        Ok(())
    }

    #[test]
    fn rich_edit_read_only_surface_blocks_plain_text_replacement() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let surface = fixture.surface();
        surface.set_text("locked text")?;
        surface.set_selection(0, 6);

        surface.set_readonly(true)?;
        let error = surface
            .replace_selection("changed")
            .expect_err("read-only replacement should be rejected");

        assert!(matches!(
            error,
            AppError::InvalidState("Text is read-only.")
        ));
        assert_eq!(surface.get_text()?, "locked text");
        Ok(())
    }

    #[test]
    fn rich_edit_modify_flag_stays_dirty_after_undo_to_loaded_text() -> Result<(), AppError> {
        let fixture = RichEditFixture::new()?;
        let surface = fixture.surface();
        surface.set_text("clean")?;
        surface.set_modified(false);
        surface.set_selection(5, 5);

        surface.replace_selection(" dirty")?;
        assert!(surface.is_modified());
        surface.undo();

        assert_eq!(surface.get_text()?, "clean");
        assert!(surface.is_modified());
        Ok(())
    }

    fn main_window_without_store() -> MainWindow {
        MainWindow {
            hwnd: null_mut(),
            tab: null_mut(),
            tab_tooltip_text: Vec::new(),
            edit: null_mut(),
            line_numbers: null_mut(),
            status: null_mut(),
            find_label: null_mut(),
            find_edit: null_mut(),
            replace_label: null_mut(),
            replace_edit: null_mut(),
            find_next_button: null_mut(),
            find_prev_button: null_mut(),
            replace_button: null_mut(),
            replace_all_button: null_mut(),
            find_close_button: null_mut(),
            find_all_button: null_mut(),
            search_results_list: null_mut(),
            command_filter: null_mut(),
            command_list: null_mut(),
            dpi_metrics: DpiMetrics::default(),
            size_move: SizeMoveDpiState::default(),
            app: EditorApp::new(),
            io: FileDocumentIo::new(),
            store: None,
            startup_paths: Vec::new(),
            startup_load_pending: false,
            startup_warnings: Vec::new(),
            last_persist_error: None,
            pending_persistence: PendingPersistence::default(),
            fixed_font: null_mut(),
            owns_font: false,
            theme_resources: ThemeResources::default(),
            programmatic_update: false,
            edit_content_pending_sync: false,
            editor_chrome_synced_after_text_change: false,
            pending_save: None,
            editor_view_states: HashMap::new(),
            visible_whitespace_display_cache: HashMap::new(),
            selection_metrics_cache: None,
            search_offset_cache: None,
            current_editor_status: CurrentEditorStatus::default(),
            status_snapshot: None,
            status_parts: Vec::new(),
            line_numbers_snapshot: None,
            editor_chrome_snapshot: None,
            create_context: null_mut(),
            show_find_bar: false,
            show_search_results: false,
            show_command_palette: false,
            show_line_numbers: true,
            command_items: Vec::new(),
            filtered_command_ids: Vec::new(),
        }
    }

    #[test]
    fn external_file_changed_buttons_map_to_actions() {
        assert_eq!(
            external_file_changed_action_from_button(EXTERNAL_CHANGE_RELOAD_BUTTON),
            ExternalFileChangedAction::Reload
        );
        assert_eq!(
            external_file_changed_action_from_button(EXTERNAL_CHANGE_SAVE_AS_BUTTON),
            ExternalFileChangedAction::SaveAs
        );
        assert_eq!(
            external_file_changed_action_from_button(EXTERNAL_CHANGE_CANCEL_BUTTON),
            ExternalFileChangedAction::Cancel
        );
        assert_eq!(
            external_file_changed_action_from_button(0),
            ExternalFileChangedAction::Cancel
        );
    }

    #[test]
    fn find_query_change_clears_stale_find_all_results() {
        let mut window = main_window_without_store();
        window.app.new_document();
        assert!(
            window
                .app
                .update_current_content("alpha beta alpha".to_string())
                .is_ok()
        );
        assert!(window.app.update_search_results("alpha").is_ok());
        window.show_search_results = true;

        window.on_find_query_changed();

        assert!(window.app.search_results().is_empty());
        assert!(!window.show_search_results);
    }

    #[test]
    fn editor_view_states_are_pruned_to_open_documents() {
        let mut window = main_window_without_store();
        let first_id = window.app.new_document();
        let second_id = window.app.new_document();
        let stale_id = DocumentId::new(999);
        window
            .editor_view_states
            .insert(first_id, EditorViewState::default());
        window
            .editor_view_states
            .insert(second_id, EditorViewState::default());
        window
            .editor_view_states
            .insert(stale_id, EditorViewState::default());

        let removed = window
            .app
            .remove_current_document()
            .expect("remove current");
        window.retain_open_editor_view_states();

        assert_eq!(removed.id(), second_id);
        assert!(window.editor_view_states.contains_key(&first_id));
        assert!(!window.editor_view_states.contains_key(&second_id));
        assert!(!window.editor_view_states.contains_key(&stale_id));
    }

    #[test]
    fn programmatic_view_change_does_not_overwrite_cached_editor_view_state() -> Result<(), AppError>
    {
        let fixture = RichEditFixture::new()?;
        let mut window = main_window_without_store();
        window.hwnd = fixture.parent;
        window.edit = fixture.edit;
        window.show_line_numbers = false;

        let document_id = window.app.new_document();
        let expected = EditorViewState {
            selection_start: 12,
            selection_end: 18,
            first_visible_line: 25,
        };
        window.editor_view_states.insert(document_id, expected);

        window.programmatic_update = true;
        window.on_editor_surface_view_changed(false);

        assert_eq!(window.editor_view_states.get(&document_id), Some(&expected));
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum SaveMode {
    Blocking {
        reload_current_document_into_edit: bool,
    },
    Background,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExternalFileChangedAction {
    Reload,
    SaveAs,
    Cancel,
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

#[derive(Clone, Copy)]
enum ShortcutMenuAction {
    Capture,
    UseDefault,
    Disable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VisibleWhitespaceDisplayCacheKey {
    document_id: DocumentId,
    content_generation: u64,
    source_len: usize,
    show_whitespace: bool,
}

impl VisibleWhitespaceDisplayCacheKey {
    fn new(
        document_id: DocumentId,
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

fn visible_whitespace_display_text<'a>(
    cache: &mut HashMap<DocumentId, VisibleWhitespaceDisplayCache>,
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

struct SelectionMetricsCache {
    document_id: DocumentId,
    text_len: usize,
    rich_selection_start: u32,
    rich_selection_end: u32,
    selected_chars: usize,
    recent_prefix_metrics: [SelectionPrefixMetric; 2],
    prefix_checkpoints: Vec<SelectionPrefixMetric>,
}

#[derive(Clone, Copy)]
struct SelectionPrefixMetric {
    rich_offset: usize,
    byte_index: usize,
    char_count: usize,
}

impl SelectionMetricsCache {
    fn new(document_id: DocumentId, text_len: usize) -> Self {
        Self {
            document_id,
            text_len,
            rich_selection_start: 0,
            rich_selection_end: 0,
            selected_chars: 0,
            recent_prefix_metrics: [SelectionPrefixMetric::zero(); 2],
            prefix_checkpoints: vec![SelectionPrefixMetric::zero()],
        }
    }

    fn nearest_prefix_checkpoint(&self, rich_offset: usize) -> SelectionPrefixMetric {
        let mut nearest = match self
            .prefix_checkpoints
            .binary_search_by_key(&rich_offset, |checkpoint| checkpoint.rich_offset)
        {
            Ok(index) => self.prefix_checkpoints[index],
            Err(0) => SelectionPrefixMetric::zero(),
            Err(index) => self.prefix_checkpoints[index - 1],
        };

        for metric in self.recent_prefix_metrics.iter().copied() {
            if metric.rich_offset <= rich_offset && metric.rich_offset >= nearest.rich_offset {
                nearest = metric;
            }
        }

        nearest
    }

    fn record_prefix_checkpoint(&mut self, metric: SelectionPrefixMetric) {
        if metric.rich_offset == 0 {
            return;
        }

        match self
            .prefix_checkpoints
            .binary_search_by_key(&metric.rich_offset, |checkpoint| checkpoint.rich_offset)
        {
            Ok(_) => {}
            Err(index) => self.prefix_checkpoints.insert(index, metric),
        }
    }

    fn record_recent_prefix_metric(&mut self, index: usize, metric: SelectionPrefixMetric) {
        if let Some(slot) = self.recent_prefix_metrics.get_mut(index) {
            *slot = metric;
        }
    }

    fn reset_prefix_metrics(&mut self) {
        self.recent_prefix_metrics = [SelectionPrefixMetric::zero(); 2];
        self.prefix_checkpoints.clear();
        self.prefix_checkpoints.push(SelectionPrefixMetric::zero());
    }
}

impl SelectionPrefixMetric {
    fn zero() -> Self {
        Self {
            rich_offset: 0,
            byte_index: 0,
            char_count: 0,
        }
    }
}

struct RichEditSearchOffsetCache {
    document_id: DocumentId,
    content: Arc<str>,
    prefix_checkpoints: Vec<RichEditSearchOffsetCheckpoint>,
}

impl RichEditSearchOffsetCache {
    fn new(document_id: DocumentId, content: Arc<str>) -> Self {
        Self {
            document_id,
            content,
            prefix_checkpoints: vec![RichEditSearchOffsetCheckpoint::zero()],
        }
    }

    fn matches(&self, document_id: DocumentId, content: &Arc<str>) -> bool {
        self.document_id == document_id && Arc::ptr_eq(&self.content, content)
    }

    fn start_checkpoint(
        &mut self,
        text: &str,
        rich_offset: usize,
    ) -> RichEditSearchOffsetCheckpoint {
        rich_edit_search_checkpoint_from_prefix(text, rich_offset, &mut self.prefix_checkpoints)
    }

    fn byte_range_to_rich_edit_offsets(
        &self,
        text: &str,
        start_byte: usize,
        end_byte: usize,
        start_checkpoint: RichEditSearchOffsetCheckpoint,
    ) -> (usize, usize) {
        byte_range_to_rich_edit_offsets_with_prefix(
            text,
            start_byte,
            end_byte,
            start_checkpoint,
            &self.prefix_checkpoints,
        )
    }

    fn byte_range_to_rich_edit_offsets_cached(
        &mut self,
        text: &str,
        start_byte: usize,
        end_byte: usize,
    ) -> (usize, usize) {
        byte_range_to_rich_edit_offsets_with_cached_prefix(
            text,
            start_byte,
            end_byte,
            &mut self.prefix_checkpoints,
        )
    }
}

struct StatusSnapshot {
    document_id: Option<DocumentId>,
    selection_start: u32,
    selection_end: u32,
    current_line: u32,
    column: u32,
    selected_chars: usize,
    char_count: usize,
    encoding: String,
    line_ending: String,
    word_wrap: bool,
    save_state: String,
    status_kind: CurrentEditorStatusKind,
    path: Option<PathBuf>,
    title: String,
    dark_theme: bool,
}

struct StatusSnapshotCandidate<'a> {
    document_id: Option<DocumentId>,
    selection_start: u32,
    selection_end: u32,
    current_line: u32,
    column: u32,
    selected_chars: usize,
    char_count: usize,
    encoding: &'a str,
    line_ending: &'a str,
    word_wrap: bool,
    save_state: &'a str,
    status_kind: CurrentEditorStatusKind,
    path: Option<&'a Path>,
    title: &'a str,
    dark_theme: bool,
}

impl StatusSnapshot {
    fn matches(&self, candidate: &StatusSnapshotCandidate<'_>) -> bool {
        self.document_id == candidate.document_id
            && self.selection_start == candidate.selection_start
            && self.selection_end == candidate.selection_end
            && self.current_line == candidate.current_line
            && self.column == candidate.column
            && self.selected_chars == candidate.selected_chars
            && self.char_count == candidate.char_count
            && self.encoding == candidate.encoding
            && self.line_ending == candidate.line_ending
            && self.word_wrap == candidate.word_wrap
            && self.save_state == candidate.save_state
            && self.status_kind == candidate.status_kind
            && self.path.as_deref() == candidate.path
            && self.title == candidate.title
            && self.dark_theme == candidate.dark_theme
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct LineNumbersSnapshot {
    first_line: usize,
    visible_count: usize,
}

struct EditorChromeSnapshot {
    status: CurrentEditorStatus,
    settings: EditorSettings,
    show_line_numbers: bool,
    show_command_palette: bool,
    editor_surface_present: bool,
    dark_theme: bool,
}

#[derive(Clone, Copy)]
struct EditorChromeSnapshotCandidate<'a> {
    status: &'a CurrentEditorStatus,
    settings: &'a EditorSettings,
    show_line_numbers: bool,
    show_command_palette: bool,
    editor_surface_present: bool,
    dark_theme: bool,
}

impl EditorChromeSnapshot {
    fn from_candidate(candidate: &EditorChromeSnapshotCandidate<'_>) -> Self {
        Self {
            status: candidate.status.clone(),
            settings: candidate.settings.clone(),
            show_line_numbers: candidate.show_line_numbers,
            show_command_palette: candidate.show_command_palette,
            editor_surface_present: candidate.editor_surface_present,
            dark_theme: candidate.dark_theme,
        }
    }

    fn matches(&self, candidate: &EditorChromeSnapshotCandidate<'_>) -> bool {
        &self.status == candidate.status
            && &self.settings == candidate.settings
            && self.show_line_numbers == candidate.show_line_numbers
            && self.show_command_palette == candidate.show_command_palette
            && self.editor_surface_present == candidate.editor_surface_present
            && self.dark_theme == candidate.dark_theme
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EditorViewState {
    selection_start: u32,
    selection_end: u32,
    first_visible_line: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorSurfaceEvent {
    TextChanged,
    SelectionChanged,
    Scrolled,
    FocusChanged,
}

#[derive(Clone, Copy)]
struct EditorTextSurface {
    hwnd: HWND,
}

impl EditorTextSurface {
    fn from_hwnd(hwnd: HWND) -> Option<Self> {
        (!hwnd.is_null()).then_some(Self { hwnd })
    }

    fn initialize_plain_text(self) -> Result<(), AppError> {
        set_rich_edit_plain_text_mode(self.hwnd)?;
        set_rich_edit_event_mask(self.hwnd);
        set_rich_edit_text_limit(self.hwnd, RICH_EDIT_EDITABLE_TEXT_LIMIT);
        Ok(())
    }

    #[cfg(test)]
    fn get_text(self) -> Result<String, AppError> {
        Ok(get_rich_edit_plain_text(self.hwnd)?.text)
    }

    fn get_text_for_document_sync(self) -> Result<RichEditPlainText, AppError> {
        let text = get_rich_edit_plain_text_for_document_sync(self.hwnd)?;
        validate_editor_document_text_size(&text.text)?;
        Ok(text)
    }

    fn set_text(self, text: &str) -> Result<(), AppError> {
        set_rich_edit_plain_text(self.hwnd, text)
    }

    fn set_text_limit(self, limit: LPARAM) {
        set_rich_edit_text_limit(self.hwnd, limit);
    }

    fn is_modified(self) -> bool {
        unsafe { SendMessageW(self.hwnd, EM_GETMODIFY_LOCAL, 0, 0) != 0 }
    }

    fn is_readonly(self) -> bool {
        unsafe { GetWindowLongPtrW(self.hwnd, GWL_STYLE) & ES_READONLY as isize != 0 }
    }

    fn set_modified(self, modified: bool) {
        unsafe {
            SendMessageW(self.hwnd, EM_SETMODIFY_LOCAL, modified as WPARAM, 0);
        }
    }

    fn set_readonly(self, read_only: bool) -> Result<(), AppError> {
        let result =
            unsafe { SendMessageW(self.hwnd, EM_SETREADONLY_LOCAL, read_only as WPARAM, 0) };
        if result == 0 {
            return Err(AppError::InvalidState("Could not set read-only mode."));
        }
        Ok(())
    }

    fn select_all(self) {
        unsafe {
            SendMessageW(self.hwnd, EM_SETSEL, 0, -1);
            SetFocus(self.hwnd);
        }
    }

    fn undo(self) {
        unsafe {
            SendMessageW(self.hwnd, WM_UNDO, 0, 0);
            SetFocus(self.hwnd);
        }
    }

    fn redo(self) {
        unsafe {
            SendMessageW(self.hwnd, EM_REDO_LOCAL, 0, 0);
            SetFocus(self.hwnd);
        }
    }

    fn can_undo(self) -> bool {
        unsafe { SendMessageW(self.hwnd, EM_CANUNDO_LOCAL, 0, 0) != 0 }
    }

    fn can_redo(self) -> bool {
        unsafe { SendMessageW(self.hwnd, EM_CANREDO_LOCAL, 0, 0) != 0 }
    }

    fn cut(self) {
        unsafe {
            SendMessageW(self.hwnd, WM_CUT, 0, 0);
            SetFocus(self.hwnd);
        }
    }

    fn copy(self) {
        unsafe {
            SendMessageW(self.hwnd, WM_COPY, 0, 0);
            SetFocus(self.hwnd);
        }
    }

    fn paste_plain_text(self) {
        unsafe {
            SendMessageW(self.hwnd, EM_PASTESPECIAL_LOCAL, CF_UNICODETEXT_LOCAL, 0);
            SetFocus(self.hwnd);
        }
    }

    fn replace_selection(self, text: &str) -> Result<(), AppError> {
        if self.is_readonly() {
            return Err(AppError::InvalidState("Text is read-only."));
        }
        replace_rich_edit_selection_plain_text(self.hwnd, text)
    }

    fn selection(self) -> (u32, u32) {
        edit_selection(self.hwnd)
    }

    fn set_selection(self, start: usize, end: usize) {
        set_edit_selection(self.hwnd, start, end);
    }

    fn view_state(self) -> EditorViewState {
        let (selection_start, selection_end) = self.selection();
        EditorViewState {
            selection_start,
            selection_end,
            first_visible_line: self.first_visible_line(),
        }
    }

    fn restore_view_state(self, state: EditorViewState) {
        unsafe {
            SendMessageW(
                self.hwnd,
                EM_SETSEL,
                state.selection_start as WPARAM,
                state.selection_end as LPARAM,
            );
        }
        self.scroll_to_first_visible_line(state.first_visible_line);
    }

    fn first_visible_line(self) -> usize {
        unsafe { SendMessageW(self.hwnd, EM_GETFIRSTVISIBLELINE, 0, 0) }.max(0) as usize
    }

    fn line_count(self) -> usize {
        unsafe { SendMessageW(self.hwnd, EM_GETLINECOUNT, 0, 0) }.max(1) as usize
    }

    fn line_and_column_from_selection_start(self, selection_start: u32) -> (u32, u32) {
        let current_line =
            unsafe { SendMessageW(self.hwnd, EM_LINEFROMCHAR, selection_start as WPARAM, 0) } + 1;
        let line_start =
            unsafe { SendMessageW(self.hwnd, EM_LINEINDEX, (current_line - 1) as WPARAM, 0) };
        let column = if line_start >= 0 {
            selection_start.saturating_sub(line_start as u32) + 1
        } else {
            1
        };
        (current_line.max(1).min(u32::MAX as isize) as u32, column)
    }

    fn scroll_to_first_visible_line(self, first_visible_line: usize) {
        let current = self.first_visible_line();
        let delta = edit_line_scroll_delta(current, first_visible_line);
        if delta == 0 {
            return;
        }
        unsafe {
            SendMessageW(self.hwnd, EM_LINESCROLL, 0, delta);
        }
    }

    fn set_tab_stops(self, tab_stop: i32) {
        unsafe {
            SendMessageW(
                self.hwnd,
                EM_SETTABSTOPS_LOCAL,
                1,
                (&tab_stop as *const i32) as LPARAM,
            );
            InvalidateRect(self.hwnd, null(), 1);
        }
    }

    fn set_word_wrap(self, word_wrap: bool) {
        let mut style = unsafe { GetWindowLongPtrW(self.hwnd, GWL_STYLE) };
        if word_wrap {
            style &= !((ES_AUTOHSCROLL | WS_HSCROLL) as isize);
        } else {
            style |= (ES_AUTOHSCROLL | WS_HSCROLL) as isize;
        }
        let wrap_width = if word_wrap { 0 } else { 1 };
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWL_STYLE, style);
            SendMessageW(self.hwnd, EM_SETTARGETDEVICE_LOCAL, 0, wrap_width);
            InvalidateRect(self.hwnd, null(), 1);
        }
    }

    fn set_background_color(self, use_system_background: bool, background: COLORREF) {
        unsafe {
            SendMessageW(
                self.hwnd,
                EM_SETBKGNDCOLOR_LOCAL,
                if use_system_background { 1 } else { 0 },
                background as LPARAM,
            );
        }
    }

    fn set_presentation_text_color(self, color: COLORREF) {
        apply_rich_edit_presentation_text_color(self.hwnd, color);
    }
}

#[derive(Clone, Copy)]
struct ThemePalette {
    editor_background: COLORREF,
    panel_background: COLORREF,
    input_background: COLORREF,
    foreground: COLORREF,
    muted_foreground: COLORREF,
    custom_controls: bool,
}

impl ThemePalette {
    const fn for_theme(theme: ThemeMode) -> Self {
        match theme {
            ThemeMode::System | ThemeMode::Light => Self {
                editor_background: rgb(255, 255, 255),
                panel_background: rgb(240, 240, 240),
                input_background: rgb(255, 255, 255),
                foreground: rgb(0, 0, 0),
                muted_foreground: rgb(96, 96, 96),
                custom_controls: false,
            },
            ThemeMode::ClassicDark => Self {
                editor_background: rgb(24, 26, 29),
                panel_background: rgb(31, 33, 36),
                input_background: rgb(24, 26, 29),
                foreground: rgb(230, 232, 235),
                muted_foreground: rgb(92, 97, 105),
                custom_controls: true,
            },
            ThemeMode::SepiaTeal => Self {
                editor_background: rgb(31, 52, 56),
                panel_background: rgb(24, 25, 24),
                input_background: rgb(31, 52, 56),
                foreground: rgb(236, 232, 219),
                muted_foreground: rgb(178, 154, 124),
                custom_controls: true,
            },
            ThemeMode::Graphite => Self {
                editor_background: rgb(50, 55, 63),
                panel_background: rgb(24, 25, 26),
                input_background: rgb(50, 55, 63),
                foreground: rgb(239, 236, 229),
                muted_foreground: rgb(126, 119, 105),
                custom_controls: true,
            },
            ThemeMode::Forest => Self {
                editor_background: rgb(39, 59, 63),
                panel_background: rgb(22, 25, 23),
                input_background: rgb(39, 59, 63),
                foreground: rgb(236, 239, 229),
                muted_foreground: rgb(104, 150, 117),
                custom_controls: true,
            },
            ThemeMode::SteelBlue => Self {
                editor_background: rgb(54, 64, 80),
                panel_background: rgb(24, 25, 27),
                input_background: rgb(54, 64, 80),
                foreground: rgb(239, 240, 242),
                muted_foreground: rgb(104, 139, 171),
                custom_controls: true,
            },
        }
    }

    const fn uses_custom_controls(self) -> bool {
        self.custom_controls
    }
}

#[derive(Default)]
struct ThemeResources {
    editor_brush: HBRUSH,
    panel_brush: HBRUSH,
    input_brush: HBRUSH,
}

struct MainWindowCreateContext {
    window_ptr: *mut MainWindow,
    owned_by_window: bool,
    create_error: Option<AppError>,
}

impl ThemeResources {
    fn recreate(&mut self, palette: ThemePalette) {
        self.release();
        self.editor_brush = unsafe { CreateSolidBrush(palette.editor_background) };
        self.panel_brush = unsafe { CreateSolidBrush(palette.panel_background) };
        self.input_brush = unsafe { CreateSolidBrush(palette.input_background) };
    }

    fn release(&mut self) {
        delete_brush(&mut self.editor_brush);
        delete_brush(&mut self.panel_brush);
        delete_brush(&mut self.input_brush);
    }
}

fn theme_command_id(theme: ThemeMode) -> u16 {
    match theme {
        ThemeMode::System => ID_THEME_SYSTEM,
        ThemeMode::Light => ID_THEME_LIGHT,
        ThemeMode::ClassicDark => ID_THEME_CLASSIC_DARK,
        ThemeMode::SepiaTeal => ID_THEME_SEPIA_TEAL,
        ThemeMode::Graphite => ID_THEME_GRAPHITE,
        ThemeMode::Forest => ID_THEME_FOREST,
        ThemeMode::SteelBlue => ID_THEME_STEEL_BLUE,
    }
}

fn theme_from_command_id(command_id: u16) -> Option<ThemeMode> {
    match command_id {
        ID_THEME_SYSTEM => Some(ThemeMode::System),
        ID_THEME_LIGHT => Some(ThemeMode::Light),
        ID_THEME_CLASSIC_DARK => Some(ThemeMode::ClassicDark),
        ID_THEME_SEPIA_TEAL => Some(ThemeMode::SepiaTeal),
        ID_THEME_GRAPHITE => Some(ThemeMode::Graphite),
        ID_THEME_FOREST => Some(ThemeMode::Forest),
        ID_THEME_STEEL_BLUE => Some(ThemeMode::SteelBlue),
        _ => None,
    }
}

fn startup_file_paths_from_args(args: impl IntoIterator<Item = OsString>) -> Vec<PathBuf> {
    args.into_iter()
        .skip(1)
        .filter(|arg| !arg.as_os_str().is_empty())
        .map(PathBuf::from)
        .collect()
}

pub fn run() -> Result<(), AppError> {
    enable_process_dpi_awareness();
    let startup_paths = startup_file_paths_from_args(env::args_os());

    let instance = unsafe { GetModuleHandleW(null()) };
    if instance.is_null() {
        return Err(last_win32_error("get module handle"));
    }

    register_main_class(instance)?;
    init_common_controls()?;
    init_rich_edit()?;

    let window = Box::new(MainWindow::new(startup_paths));
    let window_ptr = Box::into_raw(window);
    let mut create_context = MainWindowCreateContext {
        window_ptr,
        owned_by_window: false,
        create_error: None,
    };
    let startup_dpi_y = dpi_y_for_window(null_mut());
    let class_name = wide_null(CLASS_NAME);
    let title = wide_null(APP_TITLE);
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale_px_for_dpi(DEFAULT_WINDOW_WIDTH, startup_dpi_y),
            scale_px_for_dpi(DEFAULT_WINDOW_HEIGHT, startup_dpi_y),
            null_mut(),
            null_mut(),
            instance,
            (&mut create_context as *mut MainWindowCreateContext).cast(),
        )
    };

    if hwnd.is_null() {
        if !create_context.owned_by_window {
            unsafe {
                drop(Box::from_raw(window_ptr));
            }
        }
        return Err(main_window_create_failure_error(&mut create_context));
    }

    unsafe {
        ShowWindow(hwnd, SW_SHOWDEFAULT);
    }

    message_loop(hwnd)
}

pub fn report_fatal_startup_error(error: &AppError) {
    let text = wide_null(&error.user_message());
    let title = wide_null("j3Text");
    unsafe {
        MessageBoxW(
            null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn main_window_create_failure_error(create_context: &mut MainWindowCreateContext) -> AppError {
    match create_context.create_error.take() {
        Some(error) => error,
        None => last_win32_error("create main window"),
    }
}

fn register_main_class(instance: HINSTANCE) -> Result<(), AppError> {
    let class_name = wide_null(CLASS_NAME);
    let cursor = unsafe { LoadCursorW(null_mut(), IDC_ARROW) };
    if cursor.is_null() {
        return Err(last_win32_error("load cursor"));
    }
    let icon = load_app_icon(instance)?;

    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        hCursor: cursor,
        hIcon: icon,
        hIconSm: icon,
        hbrBackground: unsafe { GetSysColorBrush(COLOR_BTNFACE) },
        ..unsafe { MaybeUninit::<WNDCLASSEXW>::zeroed().assume_init() }
    };

    let atom = unsafe { RegisterClassExW(&class) };
    if atom == 0 {
        return Err(last_win32_error("register window class"));
    }

    Ok(())
}

fn load_app_icon(instance: HINSTANCE) -> Result<HICON, AppError> {
    let icon = unsafe { LoadIconW(instance, resource_id(ID_APP_ICON)) };
    if icon.is_null() {
        return Err(last_win32_error("load app icon"));
    }
    Ok(icon)
}

fn init_common_controls() -> Result<(), AppError> {
    let controls = INITCOMMONCONTROLSEX {
        dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_TAB_CLASSES | ICC_BAR_CLASSES,
    };

    let ok = unsafe { InitCommonControlsEx(&controls) };
    if ok == 0 {
        return Err(last_win32_error("initialize common controls"));
    }

    Ok(())
}

fn init_rich_edit() -> Result<(), AppError> {
    static RICH_EDIT_MODULE: OnceLock<Result<usize, u32>> = OnceLock::new();

    match RICH_EDIT_MODULE.get_or_init(|| {
        unsafe {
            SetLastError(ERROR_SUCCESS);
        }
        let module = unsafe {
            LoadLibraryExA(
                RICH_EDIT_DLL.as_ptr(),
                null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        if module.is_null() {
            Err(unsafe { GetLastError() })
        } else {
            Ok(module as usize)
        }
    }) {
        Ok(_) => Ok(()),
        Err(code) => Err(AppError::win32("load Rich Edit library", *code)),
    }
}

fn message_loop(hwnd: HWND) -> Result<(), AppError> {
    let mut message = unsafe { MaybeUninit::<MSG>::zeroed().assume_init() };
    loop {
        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if result == -1 {
            return Err(last_win32_error("get message"));
        }
        if result == 0 {
            break;
        }

        if dispatch_editor_shortcut(hwnd, &message) {
            continue;
        }

        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    let _ = hwnd;
    Ok(())
}

fn dispatch_editor_shortcut(hwnd: HWND, message: &MSG) -> bool {
    if !matches!(message.message, WM_KEYDOWN | WM_SYSKEYDOWN) {
        return false;
    }

    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow };
    if state.is_null() {
        return false;
    }

    if let Some(command_id) = unsafe { (*state).command_for_search_input_key_message(message) } {
        unsafe {
            SendMessageW(hwnd, WM_COMMAND, command_id as WPARAM, 0);
            if command_id == ID_FIND_NEXT_BUTTON {
                SetFocus((*state).find_edit);
            }
        }
        return true;
    }

    if key_message_is_repeat(message.lParam) {
        return false;
    }

    let Some(command_id) = (unsafe { (*state).command_for_shortcut_message(message) }) else {
        return false;
    };

    unsafe {
        SendMessageW(hwnd, WM_COMMAND, command_id as WPARAM, 0);
    }
    true
}

fn key_message_is_repeat(lparam: LPARAM) -> bool {
    (lparam as usize & (1usize << 30)) != 0
}

struct MainWindow {
    hwnd: HWND,
    tab: HWND,
    tab_tooltip_text: Vec<u16>,
    edit: HWND,
    line_numbers: HWND,
    status: HWND,
    find_label: HWND,
    find_edit: HWND,
    replace_label: HWND,
    replace_edit: HWND,
    find_next_button: HWND,
    find_prev_button: HWND,
    replace_button: HWND,
    replace_all_button: HWND,
    find_close_button: HWND,
    find_all_button: HWND,
    search_results_list: HWND,
    command_filter: HWND,
    command_list: HWND,
    dpi_metrics: DpiMetrics,
    size_move: SizeMoveDpiState,
    app: EditorApp,
    io: FileDocumentIo,
    store: Option<UserDataStore>,
    startup_paths: Vec<PathBuf>,
    startup_load_pending: bool,
    startup_warnings: Vec<String>,
    last_persist_error: Option<String>,
    pending_persistence: PendingPersistence,
    fixed_font: HFONT,
    owns_font: bool,
    theme_resources: ThemeResources,
    programmatic_update: bool,
    edit_content_pending_sync: bool,
    editor_chrome_synced_after_text_change: bool,
    pending_save: Option<PendingSave>,
    editor_view_states: HashMap<DocumentId, EditorViewState>,
    visible_whitespace_display_cache: HashMap<DocumentId, VisibleWhitespaceDisplayCache>,
    selection_metrics_cache: Option<SelectionMetricsCache>,
    search_offset_cache: Option<RichEditSearchOffsetCache>,
    current_editor_status: CurrentEditorStatus,
    status_snapshot: Option<StatusSnapshot>,
    status_parts: Vec<String>,
    line_numbers_snapshot: Option<LineNumbersSnapshot>,
    editor_chrome_snapshot: Option<EditorChromeSnapshot>,
    create_context: *mut MainWindowCreateContext,
    show_find_bar: bool,
    show_search_results: bool,
    show_command_palette: bool,
    show_line_numbers: bool,
    command_items: Vec<EditorCommand>,
    filtered_command_ids: Vec<EditorCommandId>,
}

impl MainWindow {
    fn new(startup_paths: Vec<PathBuf>) -> Self {
        let mut app = EditorApp::new();
        let mut startup_warnings = Vec::new();
        let store = match UserDataStore::new() {
            Ok(store) => {
                match store.load_settings() {
                    Ok(settings) => app.set_settings(settings),
                    Err(error) => startup_warnings.push(error.user_message()),
                }
                match store.load_recent_files() {
                    Ok(recent_files) => {
                        app.set_recent_files(recent_files);
                    }
                    Err(error) => startup_warnings.push(error.user_message()),
                }
                Some(store)
            }
            Err(error) => {
                startup_warnings.push(error.user_message());
                None
            }
        };

        let startup_load_pending = !startup_paths.is_empty() || !startup_warnings.is_empty();

        Self {
            hwnd: null_mut(),
            tab: null_mut(),
            tab_tooltip_text: Vec::new(),
            edit: null_mut(),
            line_numbers: null_mut(),
            status: null_mut(),
            find_label: null_mut(),
            find_edit: null_mut(),
            replace_label: null_mut(),
            replace_edit: null_mut(),
            find_next_button: null_mut(),
            find_prev_button: null_mut(),
            replace_button: null_mut(),
            replace_all_button: null_mut(),
            find_close_button: null_mut(),
            find_all_button: null_mut(),
            search_results_list: null_mut(),
            command_filter: null_mut(),
            command_list: null_mut(),
            dpi_metrics: DpiMetrics::default(),
            size_move: SizeMoveDpiState::default(),
            app,
            io: FileDocumentIo::new(),
            store,
            startup_paths,
            startup_load_pending,
            startup_warnings,
            last_persist_error: None,
            pending_persistence: PendingPersistence::default(),
            fixed_font: null_mut(),
            owns_font: false,
            theme_resources: ThemeResources::default(),
            programmatic_update: false,
            edit_content_pending_sync: false,
            editor_chrome_synced_after_text_change: false,
            pending_save: None,
            editor_view_states: HashMap::new(),
            visible_whitespace_display_cache: HashMap::new(),
            selection_metrics_cache: None,
            search_offset_cache: None,
            current_editor_status: CurrentEditorStatus::default(),
            status_snapshot: None,
            status_parts: Vec::new(),
            line_numbers_snapshot: None,
            editor_chrome_snapshot: None,
            create_context: null_mut(),
            show_find_bar: false,
            show_search_results: false,
            show_command_palette: false,
            show_line_numbers: true,
            command_items: all_commands(),
            filtered_command_ids: Vec::new(),
        }
    }

    fn editor_text_surface(&self) -> Option<EditorTextSurface> {
        EditorTextSurface::from_hwnd(self.edit)
    }

    fn command_for_search_input_key_message(&self, message: &MSG) -> Option<u16> {
        if message.message != WM_KEYDOWN {
            return None;
        }

        match (message.hwnd, message.wParam as u32) {
            (hwnd, VK_RETURN_CODE) if hwnd == self.find_edit => Some(ID_FIND_NEXT_BUTTON),
            (hwnd, VK_ESCAPE_CODE) if hwnd == self.find_edit || hwnd == self.replace_edit => {
                Some(ID_FIND_CLOSE_BUTTON)
            }
            _ => None,
        }
    }

    fn command_for_shortcut_message(&self, message: &MSG) -> Option<u16> {
        let shortcut = shortcut_from_key_message(message)?;
        let command = self.app.settings().shortcuts.command_for(shortcut)?;
        if editor_control_shortcut_command(command) && message.hwnd != self.edit {
            return None;
        }
        menu_command_id_for_editor_command(command)
    }

    fn handle_message(
        &mut self,
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_CREATE => self.handle_create(hwnd),
            WM_ENTERSIZEMOVE => {
                self.handle_enter_size_move();
                0
            }
            WM_EXITSIZEMOVE => {
                self.handle_exit_size_move_report_errors();
                0
            }
            WM_DPICHANGED => {
                if let Err(error) = self.handle_dpi_changed(wparam, lparam) {
                    show_error(hwnd, &error);
                }
                0
            }
            WM_SIZE => {
                self.layout();
                0
            }
            WM_COMMAND => self.handle_command(wparam, lparam),
            WM_NOTIFY => self.handle_notify(lparam),
            WM_CONTEXTMENU if wparam as HWND == self.edit => {
                if let Err(error) = self.show_editor_context_menu(lparam) {
                    show_error(hwnd, &error);
                }
                0
            }
            WM_ERASEBKGND => {
                if self.dark_theme_active() {
                    self.handle_erase_background(wparam)
                } else {
                    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
                }
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => self
                .handle_control_color(message, wparam, lparam)
                .unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }),
            WM_DRAWITEM => self
                .handle_draw_item(lparam)
                .unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }),
            WM_SETTINGCHANGE => {
                self.apply_theme();
                0
            }
            WM_TIMER => {
                if wparam == TIMER_STATUS {
                    match status_timer_action(self.size_move, modal_dialog_active()) {
                        StatusTimerAction::Run => self.handle_status_timer(),
                        StatusTimerAction::DeferUntilSizeMoveExit => {
                            self.size_move.defer_status_timer();
                        }
                        StatusTimerAction::SkipDuringModalDialog => return 0,
                    }
                }
                0
            }
            WM_DROPFILES => {
                self.handle_drop_files_report_errors(wparam as HDROP);
                0
            }
            WM_KEYUP | WM_LBUTTONUP | WM_VSCROLL | WM_MOUSEWHEEL => {
                if !self.consume_text_change_editor_chrome_sync(message) {
                    self.update_editor_surface_chrome_report_errors();
                }
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            }
            WM_CLOSE => {
                if unsafe { IsWindowEnabled(hwnd) == 0 } {
                    return 0;
                }
                if self.confirm_all_dirty_before_exit() {
                    self.persist_recent_files_report_errors();
                    self.flush_pending_persistence_report_errors();
                    unsafe {
                        DestroyWindow(hwnd);
                    }
                }
                0
            }
            WM_DESTROY => {
                self.stop_status_timer();
                self.persist_recent_files_report_errors();
                self.flush_pending_persistence_report_errors();
                unsafe {
                    PostQuitMessage(0);
                }
                0
            }
            WM_NCDESTROY => {
                self.release_owned_font();
                self.theme_resources.release();
                unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    fn handle_create(&mut self, hwnd: HWND) -> LRESULT {
        self.hwnd = hwnd;
        self.dpi_metrics = DpiMetrics::for_window(hwnd);
        match self.create_controls() {
            Ok(()) => 0,
            Err(error) => {
                self.capture_create_error(error);
                -1
            }
        }
    }

    fn capture_create_error(&mut self, error: AppError) {
        if !self.create_context.is_null() {
            unsafe {
                (*self.create_context).create_error = Some(error);
            }
        }
    }

    fn handle_enter_size_move(&mut self) {
        self.size_move.enter();
        self.size_move.defer_status_timer();
        self.stop_status_timer();
    }

    fn handle_exit_size_move_report_errors(&mut self) {
        if let Err(error) = self.handle_exit_size_move() {
            show_error(self.hwnd, &error);
        }
    }

    fn handle_exit_size_move(&mut self) -> Result<(), AppError> {
        let exit = self.size_move.exit();
        if exit.dpi_changed {
            self.refresh_dpi_dependent_ui(DpiMetrics::for_window(self.hwnd))?;
        }
        self.start_status_timer()?;
        if exit.status_timer_pending {
            self.handle_status_timer();
        }
        Ok(())
    }

    fn handle_status_timer(&mut self) {
        if self.startup_load_pending {
            self.startup_load_pending = false;
            self.restore_startup_state_report_errors();
            return;
        }
        self.poll_pending_save_report_errors();
        self.flush_ready_persistence_report_errors();
        self.update_editor_surface_chrome_if_changed_report_errors();
    }

    fn handle_dpi_changed(&mut self, wparam: WPARAM, lparam: LPARAM) -> Result<(), AppError> {
        if self.size_move.in_loop {
            self.size_move.mark_dpi_changed();
            return Ok(());
        }

        if let Some(rect) = suggested_rect_from_dpi_change(lparam) {
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    null_mut(),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }

        let metrics = DpiMetrics::from_wm_dpi_changed(wparam)
            .unwrap_or_else(|| DpiMetrics::for_window(self.hwnd));
        self.refresh_dpi_dependent_ui(metrics)
    }

    fn refresh_dpi_dependent_ui(&mut self, metrics: DpiMetrics) -> Result<(), AppError> {
        if self.dpi_metrics == metrics {
            return Ok(());
        }
        self.dpi_metrics = metrics;
        self.recreate_font()?;
        self.apply_font_to_controls();
        self.configure_tab_tooltips();
        self.apply_theme();
        self.layout();
        self.update_line_numbers()?;
        Ok(())
    }

    fn create_controls(&mut self) -> Result<(), AppError> {
        self.create_menu()?;

        self.recreate_font()?;
        self.theme_resources
            .recreate(ThemePalette::for_theme(self.resolved_theme()));

        self.tab = self.create_child("SysTabControl32", "", WS_TABSTOP | TCS_TOOLTIPS, 0, 0)?;
        self.configure_tab_tooltips();
        self.line_numbers = self.create_child(
            "EDIT",
            "",
            ES_RIGHT | ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL | WS_BORDER,
            0,
            0,
        )?;
        let mut edit_style = ES_LEFT | ES_MULTILINE | ES_AUTOVSCROLL | ES_NOHIDESEL | WS_VSCROLL;
        if !self.app.settings().word_wrap {
            edit_style |= ES_AUTOHSCROLL | WS_HSCROLL;
        }
        self.edit = self.create_editor_text_surface(edit_style, WS_EX_CLIENTEDGE)?;
        self.status = self.create_child("msctls_statusbar32", "", 0, 0, 0)?;

        self.find_label = self.create_child("STATIC", "Find", SS_CENTERIMAGE, 0, 0)?;
        self.find_edit =
            self.create_child("EDIT", "", ES_AUTOHSCROLL, WS_EX_CLIENTEDGE, ID_FIND_TEXT)?;
        self.replace_label = self.create_child("STATIC", "With", SS_CENTERIMAGE, 0, 0)?;
        self.replace_edit = self.create_child(
            "EDIT",
            "",
            ES_AUTOHSCROLL,
            WS_EX_CLIENTEDGE,
            ID_REPLACE_TEXT,
        )?;
        self.find_next_button = self.create_child(
            "BUTTON",
            "Next",
            BS_PUSHBUTTON as u32,
            0,
            ID_FIND_NEXT_BUTTON,
        )?;
        self.find_prev_button = self.create_child(
            "BUTTON",
            "Prev",
            BS_PUSHBUTTON as u32,
            0,
            ID_FIND_PREV_BUTTON,
        )?;
        self.replace_button =
            self.create_child("BUTTON", "One", BS_PUSHBUTTON as u32, 0, ID_REPLACE_BUTTON)?;
        self.replace_all_button = self.create_child(
            "BUTTON",
            "All",
            BS_PUSHBUTTON as u32,
            0,
            ID_REPLACE_ALL_BUTTON,
        )?;
        self.find_close_button =
            self.create_child("BUTTON", "X", BS_PUSHBUTTON as u32, 0, ID_FIND_CLOSE_BUTTON)?;
        self.find_all_button = self.create_child(
            "BUTTON",
            "List",
            BS_PUSHBUTTON as u32,
            0,
            ID_FIND_ALL_BUTTON,
        )?;
        self.search_results_list = self.create_child(
            "LISTBOX",
            "",
            LBS_NOTIFY | WS_VSCROLL | WS_BORDER,
            0,
            ID_SEARCH_RESULTS_LIST,
        )?;
        self.command_filter = self.create_child(
            "EDIT",
            "",
            ES_AUTOHSCROLL,
            WS_EX_CLIENTEDGE,
            ID_COMMAND_FILTER,
        )?;
        self.command_list = self.create_child(
            "LISTBOX",
            "",
            LBS_NOTIFY | WS_VSCROLL | WS_BORDER,
            0,
            ID_COMMAND_LIST,
        )?;

        self.apply_font_to_controls();

        self.apply_tab_stops();
        self.apply_word_wrap_style();
        self.apply_theme();

        if !self.startup_load_pending && self.app.document_count() == 0 {
            self.app.new_document();
        }
        self.refresh_tabs()?;
        self.load_current_document_into_edit()?;
        self.show_or_hide_find_bar();
        self.show_or_hide_search_results();
        self.show_or_hide_command_palette();
        self.update_command_palette_filter()?;
        self.layout();
        self.update_status()?;
        self.update_line_numbers()?;
        self.show_startup_warnings();

        self.start_status_timer()?;

        unsafe {
            DragAcceptFiles(self.hwnd, 1);
        }

        Ok(())
    }

    fn start_status_timer(&mut self) -> Result<(), AppError> {
        let timer_id = unsafe { SetTimer(self.hwnd, TIMER_STATUS, TIMER_STATUS_INTERVAL_MS, None) };
        if timer_id == 0 {
            return Err(last_win32_error("create status timer"));
        }
        Ok(())
    }

    fn stop_status_timer(&mut self) {
        if !self.hwnd.is_null() {
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_STATUS);
            }
        }
    }

    fn configure_tab_tooltips(&self) {
        if self.tab.is_null() {
            return;
        }

        let tooltip = unsafe { SendMessageW(self.tab, TCM_GETTOOLTIPS, 0, 0) as HWND };
        if tooltip.is_null() {
            return;
        }

        let max_width = self.dpi_metrics.ui_scale().px(TAB_TOOLTIP_MAX_WIDTH) as LPARAM;
        unsafe {
            SendMessageW(tooltip, TTM_SETMAXTIPWIDTH, 0, max_width);
        }
    }

    fn restore_startup_state_report_errors(&mut self) {
        if let Err(error) = self.restore_startup_state() {
            show_error(self.hwnd, &error);
        }
        self.show_startup_warnings();
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
        self.refresh_tabs()?;
        self.load_current_document_into_edit()?;
        self.layout();
        self.update_status()?;
        self.update_line_numbers()
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
        let answer = message_box(
            self.hwnd,
            &message,
            "File Missing",
            MB_YESNO | MB_ICONWARNING,
        );
        if answer != IDYES {
            return Ok(false);
        }

        self.sync_current_text()?;
        self.app.new_document_for_path(path);
        self.create_menu()?;
        self.refresh_tabs()?;
        self.load_current_document_into_edit()?;
        self.persist_recent_files_report_errors();
        Ok(true)
    }

    fn create_menu(&mut self) -> Result<(), AppError> {
        let mut menu_guard = {
            let snapshot = MainMenuSnapshot::new(self.app.settings(), self.app.recent_files());
            MainMenuBuilder::new(snapshot).build()?
        };
        install_main_menu(self.hwnd, &mut menu_guard)?;

        self.update_menu_checks();
        Ok(())
    }

    fn create_child(
        &self,
        class_name: &str,
        text: &str,
        style: u32,
        ex_style: u32,
        id: u16,
    ) -> Result<HWND, AppError> {
        self.create_child_with_context(
            class_name,
            text,
            style,
            ex_style,
            id,
            "create child control",
        )
    }

    fn create_child_with_context(
        &self,
        class_name: &str,
        text: &str,
        style: u32,
        ex_style: u32,
        id: u16,
        context: &'static str,
    ) -> Result<HWND, AppError> {
        let class_name = wide_null(class_name);
        let text = wide_null(text);
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style as WINDOW_EX_STYLE,
                class_name.as_ptr(),
                text.as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | style,
                0,
                0,
                0,
                0,
                self.hwnd,
                control_id(id),
                null_mut(),
                null_mut(),
            )
        };

        if hwnd.is_null() {
            return Err(last_win32_error(context));
        }

        Ok(hwnd)
    }

    fn create_editor_text_surface(&self, style: u32, ex_style: u32) -> Result<HWND, AppError> {
        init_rich_edit()?;
        let hwnd = self.create_child_with_context(
            RICH_EDIT_CLASS,
            "",
            style,
            ex_style,
            0,
            "create Rich Edit text surface",
        )?;
        let surface = EditorTextSurface::from_hwnd(hwnd).ok_or(AppError::InvalidState(
            "Rich Edit text surface was not created",
        ))?;
        surface.initialize_plain_text()?;
        Ok(hwnd)
    }

    fn apply_font(&self, hwnd: HWND) {
        if !hwnd.is_null() && !self.fixed_font.is_null() {
            unsafe {
                SendMessageW(hwnd, WM_SETFONT_LOCAL, self.fixed_font as WPARAM, 1);
            }
        }
    }

    fn apply_font_to_controls(&self) {
        self.apply_font(self.edit);
        self.apply_font(self.line_numbers);
        self.apply_font(self.find_edit);
        self.apply_font(self.replace_edit);
        self.apply_font(self.search_results_list);
        self.apply_font(self.command_filter);
        self.apply_font(self.command_list);
    }

    fn resolved_theme(&self) -> ThemeMode {
        match self.app.settings().theme {
            ThemeMode::System if system_prefers_dark_theme() => ThemeMode::ClassicDark,
            ThemeMode::System => ThemeMode::Light,
            theme => theme,
        }
    }

    fn active_palette(&self) -> ThemePalette {
        ThemePalette::for_theme(self.resolved_theme())
    }

    fn dark_theme_active(&self) -> bool {
        self.resolved_theme().uses_dark_mode()
    }

    fn apply_theme(&mut self) {
        let palette = self.active_palette();
        let dark = palette.uses_custom_controls();
        self.theme_resources.recreate(palette);
        // Keep dark-mode integration on documented APIs. The older uxtheme ordinal
        // entry points have no stable ABI contract, so theme support must remain
        // best-effort instead of risking a process crash during startup or redraw.
        // Native menus do not expose a documented foreground color hook, so they
        // stay system-rendered to avoid dark backgrounds with unreadable text.
        apply_non_client_dark_mode(self.hwnd, dark);
        self.apply_common_control_theme(dark);
        self.apply_menu_theme();
        self.apply_editor_text_surface_theme(palette);

        let status_color = if palette.uses_custom_controls() {
            palette.panel_background
        } else {
            CLR_DEFAULT as u32
        };
        if !self.status.is_null() {
            unsafe {
                SendMessageW(self.status, SB_SETBKCOLOR, 0, status_color as LPARAM);
            }
        }
        unsafe {
            DrawMenuBar(self.hwnd);
            SendMessageW(self.hwnd, WM_THEMECHANGED, 0, 0);
        }
        self.invalidate_theme_surfaces();
    }

    fn apply_editor_text_surface_theme(&mut self, palette: ThemePalette) {
        let Some(surface) = self.editor_text_surface() else {
            return;
        };

        let previous_programmatic_update = self.programmatic_update;
        self.programmatic_update = true;
        let use_system_background = !palette.uses_custom_controls();
        let background = if use_system_background {
            0
        } else {
            palette.editor_background
        };
        surface.set_background_color(use_system_background, background);
        surface.set_presentation_text_color(palette.foreground);
        self.programmatic_update = previous_programmatic_update;
    }

    fn apply_common_control_theme(&self, dark: bool) {
        let theme_name = dark.then(|| wide_null("DarkMode_Explorer"));
        let theme_name = theme_name
            .as_ref()
            .map(|value| value.as_ptr())
            .unwrap_or_else(null);
        for hwnd in self.themeable_controls() {
            if !hwnd.is_null() {
                unsafe {
                    let _ = SetWindowTheme(hwnd, theme_name, null());
                }
            }
        }
    }

    fn apply_menu_theme(&self) {
        let menu = unsafe { GetMenu(self.hwnd) };
        if menu.is_null() {
            return;
        }
        let info = MENUINFO {
            cbSize: size_of::<MENUINFO>() as u32,
            fMask: MIM_BACKGROUND | MIM_APPLYTOSUBMENUS,
            hbrBack: null_mut(),
            ..MENUINFO::default()
        };
        unsafe {
            SetMenuInfo(menu, &info);
        }
    }

    fn themeable_controls(&self) -> [HWND; 17] {
        [
            self.tab,
            self.edit,
            self.line_numbers,
            self.status,
            self.find_label,
            self.find_edit,
            self.replace_label,
            self.replace_edit,
            self.find_next_button,
            self.find_prev_button,
            self.replace_button,
            self.replace_all_button,
            self.find_close_button,
            self.find_all_button,
            self.search_results_list,
            self.command_filter,
            self.command_list,
        ]
    }

    fn invalidate_theme_surfaces(&self) {
        unsafe {
            InvalidateRect(self.hwnd, null(), 1);
            for hwnd in self.themeable_controls() {
                if !hwnd.is_null() {
                    InvalidateRect(hwnd, null(), 1);
                }
            }
        }
    }

    fn handle_erase_background(&self, wparam: WPARAM) -> LRESULT {
        let mut rect = RECT::default();
        let ok = unsafe { GetClientRect(self.hwnd, &mut rect) };
        if ok == 0 || self.theme_resources.panel_brush.is_null() {
            return 0;
        }
        unsafe {
            FillRect(wparam as HDC, &rect, self.theme_resources.panel_brush);
        }
        1
    }

    fn handle_control_color(
        &self,
        _message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<LRESULT> {
        let palette = self.active_palette();
        if !palette.uses_custom_controls() {
            return None;
        }

        let hdc = wparam as HDC;
        let control = lparam as HWND;
        let (text_color, background_color, brush) = if control == self.line_numbers {
            (
                palette.muted_foreground,
                palette.editor_background,
                self.theme_resources.editor_brush,
            )
        } else if control == self.edit {
            (
                palette.foreground,
                palette.editor_background,
                self.theme_resources.editor_brush,
            )
        } else if control == self.find_edit
            || control == self.replace_edit
            || control == self.command_filter
        {
            (
                palette.foreground,
                palette.input_background,
                self.theme_resources.input_brush,
            )
        } else {
            (
                palette.foreground,
                palette.panel_background,
                self.theme_resources.panel_brush,
            )
        };

        if brush.is_null() {
            return None;
        }
        unsafe {
            SetTextColor(hdc, text_color);
            SetBkColor(hdc, background_color);
        }
        Some(brush as LRESULT)
    }

    fn handle_draw_item(&self, lparam: LPARAM) -> Option<LRESULT> {
        if lparam == 0 || !self.dark_theme_active() {
            return None;
        }
        let draw = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
        if draw.hwndItem != self.status {
            return None;
        }
        let text = self.status_parts.get(draw.itemData)?;
        self.draw_dark_status_part(draw, text);
        Some(1)
    }

    fn draw_dark_status_part(&self, draw: &DRAWITEMSTRUCT, text: &str) {
        if self.theme_resources.panel_brush.is_null() {
            return;
        }

        let palette = self.active_palette();
        let mut rect = draw.rcItem;
        unsafe {
            FillRect(draw.hDC, &rect, self.theme_resources.panel_brush);
            SetTextColor(draw.hDC, palette.foreground);
            SetBkMode(draw.hDC, TRANSPARENT as i32);
        }
        rect.left += 4;
        let text = wide_null(text);
        unsafe {
            DrawTextW(
                draw.hDC,
                text.as_ptr(),
                -1,
                &mut rect,
                DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
            );
        }
    }

    fn handle_command(&mut self, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let command_id = loword(wparam);
        let notify_code = hiword(wparam);
        let source = lparam as HWND;

        if source == self.edit
            && let Some(event) = match notify_code as u32 {
                EN_CHANGE => Some(EditorSurfaceEvent::TextChanged),
                EN_HSCROLL | EN_VSCROLL => Some(EditorSurfaceEvent::Scrolled),
                EN_SETFOCUS | EN_KILLFOCUS => Some(EditorSurfaceEvent::FocusChanged),
                _ => None,
            }
        {
            self.on_editor_surface_event_report_errors(event);
            return 0;
        }
        if source == self.find_edit && notify_code == EN_CHANGE as u16 {
            self.on_find_query_changed();
            return 0;
        }
        if source == self.command_filter && notify_code == EN_CHANGE as u16 {
            if let Err(error) = self.update_command_palette_filter() {
                show_error(self.hwnd, &error);
            }
            return 0;
        }
        if source == self.search_results_list && notify_code == LBN_DBLCLK {
            if let Err(error) = self.activate_selected_search_result() {
                show_error(self.hwnd, &error);
            }
            return 0;
        }
        if source == self.command_list && notify_code == LBN_DBLCLK {
            if let Err(error) = self.activate_selected_command() {
                self.report_error(error);
            }
            return 0;
        }

        let result = if let Some(index) = recent_index_from_command(command_id) {
            self.open_recent_file(index)
        } else if let Some((command, action)) = shortcut_settings_from_command(command_id) {
            self.configure_shortcut(command, action)
        } else {
            self.handle_fixed_command(command_id)
        };

        if let Err(error) = result {
            self.report_error(error);
        }

        0
    }

    fn handle_fixed_command(&mut self, command_id: u16) -> Result<(), AppError> {
        if let Some(theme) = theme_from_command_id(command_id) {
            return self.set_theme(theme);
        }

        if self.handle_file_command(command_id)? {
            return Ok(());
        }
        if self.handle_edit_command(command_id)? {
            return Ok(());
        }
        if self.handle_search_command(command_id)? {
            return Ok(());
        }
        if self.handle_view_command(command_id)? {
            return Ok(());
        }
        if self.handle_document_command(command_id)? {
            return Ok(());
        }
        if self.handle_settings_command(command_id)? {
            return Ok(());
        }
        if self.handle_help_command(command_id)? {
            return Ok(());
        }

        Ok(())
    }

    fn handle_file_command(&mut self, command_id: u16) -> Result<bool, AppError> {
        match command_id {
            ID_FILE_NEW => self.new_tab(),
            ID_FILE_OPEN => self.open_file(None),
            ID_FILE_SAVE => self.save_current_command(false),
            ID_FILE_SAVE_AS => self.save_current_command(true),
            ID_FILE_CLOSE_TAB => self.close_current_tab(),
            ID_FILE_CLOSE_OTHER_TABS => self.close_other_tabs(),
            ID_FILE_CLOSE_ALL_TABS => self.close_all_tabs(),
            ID_FILE_EXIT => {
                unsafe {
                    SendMessageW(self.hwnd, WM_CLOSE, 0, 0);
                }
                Ok(())
            }
            _ => return Ok(false),
        }?;

        Ok(true)
    }

    fn handle_edit_command(&mut self, command_id: u16) -> Result<bool, AppError> {
        match command_id {
            ID_EDIT_UNDO => self.text_surface_command(true, EditorTextSurface::undo),
            ID_EDIT_REDO => self.text_surface_command(true, EditorTextSurface::redo),
            ID_EDIT_CUT => self.text_surface_command(true, EditorTextSurface::cut),
            ID_EDIT_COPY => self.text_surface_command(false, EditorTextSurface::copy),
            ID_EDIT_PASTE => self.paste_plain_text(),
            ID_EDIT_SELECT_ALL => {
                self.select_all_text();
                Ok(())
            }
            _ => return Ok(false),
        }?;

        Ok(true)
    }

    fn handle_search_command(&mut self, command_id: u16) -> Result<bool, AppError> {
        match command_id {
            ID_EDIT_FIND => {
                self.show_find_bar_and_focus(self.find_edit);
                Ok(())
            }
            ID_EDIT_REPLACE => {
                self.show_find_bar_and_focus(self.replace_edit);
                Ok(())
            }
            ID_EDIT_FIND_NEXT | ID_FIND_NEXT_BUTTON => self.search(SearchDirection::Forward),
            ID_EDIT_FIND_PREVIOUS | ID_FIND_PREV_BUTTON => self.search(SearchDirection::Backward),
            ID_EDIT_FIND_ALL | ID_FIND_ALL_BUTTON => self.find_all_results(),
            ID_REPLACE_BUTTON => self.replace_current(),
            ID_REPLACE_ALL_BUTTON => self.replace_all(),
            ID_FIND_CLOSE_BUTTON => {
                self.close_find_bar_and_results();
                Ok(())
            }
            _ => return Ok(false),
        }?;

        Ok(true)
    }

    fn show_find_bar_and_focus(&mut self, focus_target: HWND) {
        self.show_find_bar = true;
        self.show_or_hide_find_bar();
        self.layout();
        unsafe {
            SetFocus(focus_target);
        }
    }

    fn close_find_bar_and_results(&mut self) {
        self.show_find_bar = false;
        self.show_search_results = false;
        self.show_or_hide_find_bar();
        self.show_or_hide_search_results();
        self.layout();
    }

    fn handle_view_command(&mut self, command_id: u16) -> Result<bool, AppError> {
        match command_id {
            ID_VIEW_LINE_NUMBERS => {
                self.show_line_numbers = !self.show_line_numbers;
                self.layout();
                self.update_menu_checks();
                Ok(())
            }
            ID_VIEW_WHITESPACE => self.toggle_visible_whitespace(),
            ID_VIEW_COMMAND_PALETTE => self.toggle_command_palette(),
            _ => return Ok(false),
        }?;

        Ok(true)
    }

    fn handle_document_command(&mut self, command_id: u16) -> Result<bool, AppError> {
        match command_id {
            ID_ENCODING_REOPEN => self.reopen_current_file_with_encoding_dialog(),
            ID_ENCODING_CONVERT => self.convert_current_encoding_dialog(),
            ID_LINE_ENDING_CRLF => self.set_line_ending(LineEnding::Crlf),
            ID_LINE_ENDING_LF => self.set_line_ending(LineEnding::Lf),
            ID_LINE_ENDING_CR => self.set_line_ending(LineEnding::Cr),
            ID_TAB_MOVE_LEFT => self.move_current_tab_left(),
            ID_TAB_MOVE_RIGHT => self.move_current_tab_right(),
            ID_TAB_OPEN_NEW_WINDOW => self.open_current_tab_in_new_window(),
            _ => return Ok(false),
        }?;

        Ok(true)
    }

    fn handle_settings_command(&mut self, command_id: u16) -> Result<bool, AppError> {
        match command_id {
            ID_SETTINGS_CHOOSE_FONT => self.choose_font(),
            ID_SETTINGS_TAB_SIZE_2 => self.set_tab_size(2),
            ID_SETTINGS_TAB_SIZE_4 => self.set_tab_size(4),
            ID_SETTINGS_TAB_SIZE_8 => self.set_tab_size(8),
            ID_SETTINGS_WORD_WRAP => self.toggle_word_wrap(),
            _ => return Ok(false),
        }?;

        Ok(true)
    }

    fn handle_help_command(&mut self, command_id: u16) -> Result<bool, AppError> {
        match command_id {
            ID_HELP_ABOUT => {
                show_about_dialog(self.hwnd);
                Ok(())
            }
            _ => return Ok(false),
        }?;

        Ok(true)
    }

    fn handle_notify(&mut self, lparam: LPARAM) -> LRESULT {
        if lparam == 0 {
            return 0;
        }

        let header = unsafe { &*(lparam as *const windows_sys::Win32::UI::Controls::NMHDR) };
        if header.code == TTN_GETDISPINFOW && self.is_tab_tooltip(header.hwndFrom) {
            self.handle_tab_tooltip_disp_info(lparam);
            return 0;
        }

        if header.hwndFrom == self.edit && header.code == EN_SELCHANGE_LOCAL {
            self.on_editor_surface_event_report_errors(EditorSurfaceEvent::SelectionChanged);
            return 0;
        }

        if header.hwndFrom == self.status
            && header.code == NM_CUSTOMDRAW
            && self.dark_theme_active()
        {
            let palette = self.active_palette();
            let draw = unsafe { &*(lparam as *const NMCUSTOMDRAW) };
            return match draw.dwDrawStage {
                CDDS_PREPAINT => CDRF_NOTIFYITEMDRAW as LRESULT,
                CDDS_ITEMPREPAINT => {
                    unsafe {
                        SetTextColor(draw.hdc, palette.foreground);
                        SetBkColor(draw.hdc, palette.panel_background);
                    }
                    CDRF_NEWFONT as LRESULT
                }
                _ => 0,
            };
        }

        if header.hwndFrom == self.tab && header.code == TCN_SELCHANGE {
            let selected = unsafe { SendMessageW(self.tab, TCM_GETCURSEL, 0, 0) };
            if selected >= 0 {
                let result = self.select_tab(selected as usize);
                if let Err(error) = result {
                    if let Some(index) = self.app.current_index() {
                        self.select_tab_control_item(index);
                    }
                    show_error(self.hwnd, &error);
                } else {
                    self.show_search_results = false;
                    self.show_or_hide_search_results();
                    self.layout();
                }
            }
        }

        if header.hwndFrom == self.tab && header.code == NM_RCLICK {
            if let Err(error) = self.show_tab_context_menu() {
                show_error(self.hwnd, &error);
            }
            return 0;
        }

        0
    }

    fn is_tab_tooltip(&self, hwnd: HWND) -> bool {
        if self.tab.is_null() || hwnd.is_null() {
            return false;
        }

        let tooltip = unsafe { SendMessageW(self.tab, TCM_GETTOOLTIPS, 0, 0) as HWND };
        !tooltip.is_null() && hwnd == tooltip
    }

    fn handle_tab_tooltip_disp_info(&mut self, lparam: LPARAM) {
        let Some(info) = (unsafe { (lparam as *mut NMTTDISPINFOW).as_mut() }) else {
            return;
        };

        self.tab_tooltip_text = self
            .tab_tooltip_path_at_cursor()
            .map(|path| wide_null(&path))
            .unwrap_or_else(|| vec![0]);
        info.lpszText = self.tab_tooltip_text.as_mut_ptr();
    }

    fn tab_tooltip_path_at_cursor(&self) -> Option<String> {
        let (index, _) = match self.tab_index_at_cursor() {
            Ok(Some(value)) => value,
            Ok(None) | Err(_) => return None,
        };
        self.app
            .documents()
            .get(index)
            .and_then(|document| document.path())
            .map(|path| Self::format_tab_tooltip_path(path.as_path()))
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

    fn new_tab(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        self.app.new_document();
        self.refresh_tabs()?;
        self.load_current_document_into_edit()?;
        Ok(())
    }

    fn open_file(&mut self, requested_encoding: Option<TextEncoding>) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        if !self.confirm_current_dirty("open")? {
            return Ok(());
        }

        if let Some(path) = open_file_dialog(self.hwnd)? {
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
            self.create_menu()?;
            message_box(
                self.hwnd,
                "Recent file is gone. It was removed.",
                "Recent",
                MB_OK | MB_ICONWARNING,
            );
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
            self.create_menu()?;
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
        self.create_menu()?;
        self.refresh_tabs()?;
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
        self.create_menu()?;
        self.refresh_current_tab_display()?;
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
        self.create_menu()?;
        self.refresh_current_tab_display()?;
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
            error => show_error(self.hwnd, &error),
        }
    }

    fn resolve_external_file_changed(&mut self, path: PathBuf) -> Result<(), AppError> {
        let Some(index) = self.document_index_for_path(&path) else {
            show_error(self.hwnd, &AppError::external_file_changed(path));
            return Ok(());
        };
        if self.app.current_index() != Some(index) {
            self.select_tab(index)?;
        }

        match external_file_changed_action_dialog(self.hwnd, &path) {
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
        let Some(encoding) = choose_encoding_dialog(
            self.hwnd,
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

    fn convert_current_encoding_dialog(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        self.ensure_current_writable()?;
        let current = self
            .app
            .current_document()
            .ok_or(AppError::InvalidState("No file open."))?
            .encoding();
        let Some(encoding) = choose_encoding_dialog(
            self.hwnd,
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
            message_box(
                self.hwnd,
                &format!("{}\n\nPick another encoding.", error.user_message()),
                "Encoding",
                MB_OK | MB_ICONWARNING,
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
            None => match save_file_dialog(self.hwnd)? {
                Some(path) => path,
                None => return Ok(false),
            },
        };
        let selected_target_expectation = if needs_save_as {
            Some(selected_save_target_expectation(
                &self.io,
                save_path.as_path(),
            )?)
        } else {
            None
        };
        if needs_save_as {
            let Some(selected_encoding) = choose_encoding_dialog(
                self.hwnd,
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
                self.create_menu()?;
                self.refresh_current_tab_display()?;
                if reload_current_document_into_edit {
                    self.load_current_document_into_edit()?;
                } else {
                    self.refresh_preserved_edit_after_blocking_save()?;
                }
                self.update_status()?;
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

    fn refresh_preserved_edit_after_blocking_save(&mut self) -> Result<(), AppError> {
        if let Some(surface) = self.editor_text_surface()
            && surface.is_modified()
        {
            surface.set_modified(false);
        }
        self.edit_content_pending_sync = false;
        self.apply_current_read_only()?;
        self.update_line_numbers()?;
        self.update_menu_checks();
        Ok(())
    }

    fn start_background_save(
        &mut self,
        document_id: DocumentId,
        path: PathBuf,
        encoding: TextEncoding,
        line_ending: LineEnding,
        target_expectation: SaveTargetExpectation,
        content: std::sync::Arc<str>,
    ) -> Result<(), AppError> {
        let (sender, receiver) = mpsc::channel();
        let worker_path = path.clone();
        let _save_worker = thread::Builder::new()
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
        self.apply_current_read_only()?;
        self.update_status()?;
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
            Ok(saved) => self.complete_successful_pending_save(pending, saved),
            Err(error) => self.complete_failed_pending_save(error),
        }
    }

    fn complete_successful_pending_save(
        &mut self,
        pending: PendingSave,
        saved: SavedFileSnapshot,
    ) -> Result<(), AppError> {
        self.app.mark_document_saved(
            pending.document_id,
            pending.path,
            pending.encoding,
            pending.line_ending,
            Some(saved.snapshot),
        )?;
        self.create_menu()?;
        self.refresh_tab_display_for_document(pending.document_id)?;
        self.refresh_after_pending_save_completion()?;
        self.persist_recent_files_report_errors();
        self.flush_pending_persistence_report_errors();
        Ok(())
    }

    fn complete_failed_pending_save(&mut self, error: AppError) -> Result<(), AppError> {
        self.refresh_after_pending_save_completion()?;
        Err(error)
    }

    fn refresh_after_pending_save_completion(&mut self) -> Result<(), AppError> {
        self.apply_current_read_only()?;
        self.update_status()?;
        self.update_menu_checks();
        Ok(())
    }

    fn show_editor_context_menu(&mut self, lparam: LPARAM) -> Result<(), AppError> {
        if self.editor_text_surface().is_none() {
            return Ok(());
        }

        let screen_point = context_menu_screen_point(lparam, self.edit)?;
        let menu_guard = self.build_editor_context_menu()?;
        unsafe {
            SetFocus(self.edit);
        }

        let selected = unsafe {
            TrackPopupMenu(
                menu_guard.handle(),
                TPM_RIGHTBUTTON | TPM_RETURNCMD,
                screen_point.x,
                screen_point.y,
                0,
                self.hwnd,
                null(),
            )
        } as u16;

        if selected == 0 {
            return Ok(());
        }
        self.handle_fixed_command(selected)
    }

    fn build_editor_context_menu(&mut self) -> Result<MenuGuard, AppError> {
        self.with_refreshed_current_editor_status(|window, status| {
            window.build_editor_context_menu_from_status(status)
        })
    }

    fn build_editor_context_menu_from_status(
        &self,
        status: &CurrentEditorStatus,
    ) -> Result<MenuGuard, AppError> {
        let guard = MenuGuard::create_popup("create editor context menu")?;
        let menu = guard.handle();
        let shortcuts = &self.app.settings().shortcuts;

        append_item(
            menu,
            ID_EDIT_UNDO,
            &command_menu_label("Undo", shortcuts.shortcut_for(EditorCommandId::Undo)),
        )?;
        append_item(
            menu,
            ID_EDIT_REDO,
            &command_menu_label("Redo", shortcuts.shortcut_for(EditorCommandId::Redo)),
        )?;
        append_separator(menu)?;
        append_item(
            menu,
            ID_EDIT_CUT,
            &command_menu_label("Cut", shortcuts.shortcut_for(EditorCommandId::Cut)),
        )?;
        append_item(
            menu,
            ID_EDIT_COPY,
            &command_menu_label("Copy", shortcuts.shortcut_for(EditorCommandId::Copy)),
        )?;
        append_item(
            menu,
            ID_EDIT_PASTE,
            &command_menu_label("Paste", shortcuts.shortcut_for(EditorCommandId::Paste)),
        )?;
        append_item(
            menu,
            ID_EDIT_SELECT_ALL,
            &command_menu_label(
                "Select All",
                shortcuts.shortcut_for(EditorCommandId::SelectAll),
            ),
        )?;

        let editor_available = status.document_id.is_some() && self.editor_text_surface().is_some();
        enable_menu_item(menu, ID_EDIT_UNDO, status.can_undo);
        enable_menu_item(menu, ID_EDIT_REDO, status.can_redo);
        enable_menu_item(menu, ID_EDIT_CUT, status.can_edit);
        enable_menu_item(menu, ID_EDIT_COPY, editor_available);
        enable_menu_item(menu, ID_EDIT_PASTE, status.can_edit);
        enable_menu_item(menu, ID_EDIT_SELECT_ALL, editor_available);

        Ok(guard)
    }

    fn show_tab_context_menu(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        let Some((tab_index, screen_point)) = self.tab_index_at_cursor()? else {
            return Ok(());
        };

        self.select_tab(tab_index)?;

        let menu = unsafe { CreatePopupMenu() };
        if menu.is_null() {
            return Err(last_win32_error("create tab context menu"));
        }

        let build_result = (|| -> Result<(), AppError> {
            if self.app.document_count() > 1 {
                append_item(menu, ID_TAB_OPEN_NEW_WINDOW, "Open in New Window")?;
            } else {
                append_disabled_item(menu, "Open in New Window")?;
            }
            append_separator(menu)?;
            append_item(
                menu,
                ID_FILE_CLOSE_TAB,
                &command_menu_label(
                    "Close",
                    self.app
                        .settings()
                        .shortcuts
                        .shortcut_for(EditorCommandId::CloseTab),
                ),
            )?;
            append_item(menu, ID_FILE_CLOSE_OTHER_TABS, "Close Others")?;
            Ok(())
        })();

        if let Err(error) = build_result {
            unsafe {
                DestroyMenu(menu);
            }
            return Err(error);
        }

        let selected = unsafe {
            TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON | TPM_RETURNCMD,
                screen_point.x,
                screen_point.y,
                0,
                self.hwnd,
                null(),
            )
        } as u16;
        unsafe {
            DestroyMenu(menu);
        }

        match selected {
            ID_TAB_OPEN_NEW_WINDOW => self.open_current_tab_in_new_window(),
            ID_FILE_CLOSE_TAB => self.close_current_tab(),
            ID_FILE_CLOSE_OTHER_TABS => self.close_other_tabs(),
            _ => Ok(()),
        }
    }

    fn tab_index_at_cursor(&self) -> Result<Option<(usize, POINT)>, AppError> {
        let mut screen_point = POINT::default();
        let got_cursor = unsafe { GetCursorPos(&mut screen_point) };
        if got_cursor == 0 {
            return Err(last_win32_error("get cursor position"));
        }

        let mut client_point = screen_point;
        let converted = unsafe { ScreenToClient(self.tab, &mut client_point) };
        if converted == 0 {
            return Err(last_win32_error("convert cursor position"));
        }

        let mut hit = TCHITTESTINFO {
            pt: client_point,
            flags: 0,
        };
        let index = unsafe {
            SendMessageW(
                self.tab,
                TCM_HITTEST,
                0,
                (&mut hit as *mut TCHITTESTINFO) as LPARAM,
            )
        };
        if index < 0 {
            return Ok(None);
        }

        Ok(Some((index as usize, screen_point)))
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
            self.remove_editor_view_state(document.id());
            self.remove_visible_whitespace_display_cache(document.id());
        }
        if self.app.document_count() == 0 {
            self.app.new_document();
        }
        self.refresh_tabs()?;
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

        let dirty_indices: Vec<usize> = self
            .app
            .dirty_indices()
            .into_iter()
            .filter(|index| *index != current_index)
            .collect();
        if !self.confirm_dirty_indices(&dirty_indices, "close")? {
            self.select_tab(current_index)?;
            return Ok(());
        }

        self.select_tab(current_index)?;
        if self.app.remove_other_documents() {
            self.retain_open_editor_view_states();
            self.retain_open_visible_whitespace_display_cache();
            self.refresh_tabs()?;
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
        self.refresh_tabs()?;
        self.load_current_document_into_edit()?;
        self.persist_recent_files_report_errors();
        Ok(())
    }

    fn set_encoding(&mut self, encoding: TextEncoding) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.ensure_current_writable()?;
        self.app.set_current_encoding(encoding)?;
        self.refresh_current_tab_display()?;
        self.update_status()?;
        self.update_menu_checks();
        Ok(())
    }

    fn set_line_ending(&mut self, line_ending: LineEnding) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.ensure_current_writable()?;
        self.app.set_current_line_ending(line_ending)?;
        self.refresh_current_tab_display()?;
        self.update_status()?;
        self.update_menu_checks();
        Ok(())
    }

    fn refresh_current_editor_surface_state(&mut self) {
        let document_id = self.app.current_document().map(|document| document.id());
        let state = self
            .editor_text_surface()
            .map(|surface| {
                let (selection_start, selection_end) = surface.selection();
                let (line, column) = surface.line_and_column_from_selection_start(selection_start);
                EditorSurfaceState::from_surface(
                    document_id,
                    selection_start,
                    selection_end,
                    line,
                    column,
                    surface.can_undo(),
                    surface.can_redo(),
                )
            })
            .unwrap_or_else(|| EditorSurfaceState::for_document(document_id));
        self.app.update_current_editor_surface_state(state);
    }

    fn sync_current_text(&mut self) -> Result<(), AppError> {
        let Some(surface) = self.editor_text_surface() else {
            return Ok(());
        };
        if self.programmatic_update {
            return Ok(());
        }
        self.remember_current_view_state();
        if self.app.settings().show_whitespace {
            return Ok(());
        }
        if !self.edit_content_pending_sync {
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

        let text = surface.get_text_for_document_sync()?;
        self.app
            .update_current_changed_view_content_with_metrics(text.text, text.metrics)?;
        self.edit_content_pending_sync = false;
        self.invalidate_selection_metrics_cache();
        self.invalidate_search_offset_cache();
        Ok(())
    }

    fn remember_current_view_state(&mut self) {
        let Some(surface) = self.editor_text_surface() else {
            return;
        };
        let Some(document_id) = self.app.current_document().map(|document| document.id()) else {
            return;
        };
        self.editor_view_states
            .insert(document_id, surface.view_state());
    }

    fn restore_current_view_state(&mut self) {
        let Some(surface) = self.editor_text_surface() else {
            return;
        };
        let Some(document_id) = self.app.current_document().map(|document| document.id()) else {
            return;
        };
        let Some(state) = self.editor_view_states.get(&document_id).copied() else {
            return;
        };
        surface.restore_view_state(state);
        self.invalidate_selection_metrics_cache();
        self.line_numbers_snapshot = None;
    }

    fn remove_editor_view_state(&mut self, document_id: DocumentId) {
        self.editor_view_states.remove(&document_id);
    }

    fn remove_visible_whitespace_display_cache(&mut self, document_id: DocumentId) {
        self.visible_whitespace_display_cache.remove(&document_id);
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

    fn on_editor_surface_event_report_errors(&mut self, event: EditorSurfaceEvent) {
        match event {
            EditorSurfaceEvent::TextChanged => self.on_editor_surface_text_changed_report_errors(),
            EditorSurfaceEvent::SelectionChanged => self.on_editor_surface_view_changed(true),
            EditorSurfaceEvent::Scrolled => self.on_editor_surface_view_changed(false),
            EditorSurfaceEvent::FocusChanged => self.on_editor_surface_focus_changed(),
        }
    }

    fn on_editor_surface_text_changed_report_errors(&mut self) {
        self.editor_chrome_synced_after_text_change = false;
        if self.programmatic_update {
            return;
        }
        if self.current_document_is_saving() {
            return;
        }

        self.edit_content_pending_sync = true;
        self.invalidate_selection_metrics_cache();
        self.invalidate_search_offset_cache();
        let result = self
            .app
            .mark_current_dirty_from_view()
            .and_then(|dirty_changed| {
                self.app.clear_search_results();
                self.show_search_results = false;
                self.show_or_hide_search_results();
                if dirty_changed {
                    self.refresh_current_tab_display()?;
                }
                self.update_editor_surface_chrome()?;
                self.editor_chrome_synced_after_text_change = true;
                Ok(())
            });
        if let Err(error) = result {
            show_error(self.hwnd, &error);
        }
    }

    fn on_editor_surface_view_changed(&mut self, _selection_changed: bool) {
        if self.programmatic_update {
            return;
        }
        self.remember_current_view_state();
        self.with_refreshed_current_editor_status(|window, status| {
            if let Err(error) = window
                .update_status_from_current_editor_status(status)
                .and_then(|()| window.update_line_numbers())
            {
                show_error(window.hwnd, &error);
            }
            window.update_menu_checks_from_current_editor_status(status);
        });
    }

    fn on_editor_surface_focus_changed(&mut self) {
        self.with_refreshed_current_editor_status(|window, status| {
            if let Err(error) = window.update_status_from_current_editor_status(status) {
                show_error(window.hwnd, &error);
            }
            window.update_menu_checks_from_current_editor_status(status);
        });
    }

    fn on_find_query_changed(&mut self) {
        if self.show_search_results || !self.app.search_results().is_empty() {
            self.app.clear_search_results();
            self.show_search_results = false;
            self.show_or_hide_search_results();
            self.layout();
        }
    }

    fn refresh_tabs(&mut self) -> Result<(), AppError> {
        unsafe {
            SendMessageW(self.tab, TCM_DELETEALLITEMS, 0, 0);
        }

        for (index, document) in self.app.documents().iter().enumerate() {
            let _document_id = document.id();
            let mut title = wide_null(&document.tab_title());
            let mut item = TCITEMW {
                mask: TCIF_TEXT,
                pszText: title.as_mut_ptr(),
                ..unsafe { MaybeUninit::<TCITEMW>::zeroed().assume_init() }
            };
            let inserted = unsafe {
                SendMessageW(
                    self.tab,
                    TCM_INSERTITEMW,
                    index,
                    (&mut item as *mut TCITEMW) as LPARAM,
                )
            };
            if inserted == -1 {
                return Err(last_win32_error("insert tab"));
            }
        }

        if let Some(index) = self.app.current_index() {
            self.select_tab_control_item(index);
        } else {
            self.update_menu_checks();
        }
        Ok(())
    }

    fn refresh_current_tab_display(&mut self) -> Result<(), AppError> {
        if let Some(index) = self.app.current_index() {
            self.refresh_tab_display_at(index)?;
        }
        self.update_menu_checks();
        Ok(())
    }

    fn refresh_tab_display_for_document(
        &mut self,
        document_id: DocumentId,
    ) -> Result<(), AppError> {
        let index = self
            .app
            .documents()
            .iter()
            .position(|document| document.id() == document_id)
            .ok_or(AppError::InvalidState("Tab not found."))?;
        self.refresh_tab_display_at(index)?;
        self.update_menu_checks();
        Ok(())
    }

    fn refresh_tab_display_at(&mut self, index: usize) -> Result<(), AppError> {
        let document = self
            .app
            .documents()
            .get(index)
            .ok_or(AppError::InvalidState("Tab not found."))?;
        let mut title = wide_null(&document.tab_title());
        let mut item = TCITEMW {
            mask: TCIF_TEXT,
            pszText: title.as_mut_ptr(),
            ..unsafe { MaybeUninit::<TCITEMW>::zeroed().assume_init() }
        };
        let updated = unsafe {
            SendMessageW(
                self.tab,
                TCM_SETITEMW,
                index,
                (&mut item as *mut TCITEMW) as LPARAM,
            )
        };
        if updated == 0 {
            return Err(last_win32_error("update tab"));
        }
        Ok(())
    }

    fn select_tab_control_item(&mut self, index: usize) {
        unsafe {
            SendMessageW(self.tab, TCM_SETCURSEL, index, 0);
        }
        self.update_menu_checks();
    }

    fn load_current_document_into_edit(&mut self) -> Result<(), AppError> {
        if self.app.settings().show_whitespace
            && self.app.current_document().is_some_and(|document| {
                !can_render_visible_whitespace_bytes(document.content().len())
            })
        {
            let mut settings = self.app.settings().clone();
            settings.show_whitespace = false;
            self.app.set_settings(settings);
        }

        let show_whitespace = self.app.settings().show_whitespace;
        let content = match self.app.current_document() {
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
        let result = self
            .editor_text_surface()
            .map(|surface| {
                surface.set_text_limit(RICH_EDIT_SURFACE_TEXT_LIMIT);
                surface.set_text(content.as_str())
            })
            .unwrap_or(Ok(()));
        self.programmatic_update = false;
        result?;
        if let Some(surface) = self.editor_text_surface()
            && surface.is_modified()
        {
            surface.set_modified(false);
        }
        self.edit_content_pending_sync = false;
        self.invalidate_selection_metrics_cache();
        self.invalidate_search_offset_cache();
        self.restore_current_view_state();
        self.apply_current_read_only()?;
        self.update_status()?;
        self.update_line_numbers()?;
        self.update_menu_checks();
        Ok(())
    }

    fn confirm_current_dirty(&mut self, action: &str) -> Result<bool, AppError> {
        self.sync_current_text()?;
        let is_dirty = self
            .app
            .current_document()
            .is_some_and(|document| document.is_dirty());
        if !is_dirty {
            return Ok(true);
        }

        let message = format!("Unsaved changes.\nSave before {action}?");
        match message_box(
            self.hwnd,
            &message,
            "Unsaved",
            MB_YESNOCANCEL | MB_ICONWARNING,
        ) {
            IDYES => self.save_current_for_dirty_prompt(false),
            IDNO => Ok(true),
            _ => Ok(false),
        }
    }

    fn confirm_all_dirty_before_exit(&mut self) -> bool {
        if let Err(error) = self.poll_pending_save() {
            self.report_error(error);
            return false;
        }
        if self.pending_save.is_some() {
            message_box(
                self.hwnd,
                "Save is still running. Please wait.",
                "Saving",
                MB_OK | MB_ICONINFORMATION,
            );
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

    fn select_tab(&mut self, index: usize) -> Result<(), AppError> {
        let previous_index = self.app.current_index();
        let refresh_previous_tab = self.edit_content_pending_sync;
        self.sync_current_text()?;
        if refresh_previous_tab && let Some(previous_index) = previous_index {
            self.refresh_tab_display_at(previous_index)?;
        }
        self.app.set_current_index(index)?;
        self.select_tab_control_item(index);
        self.refresh_current_tab_display()?;
        self.load_current_document_into_edit()
    }

    fn search(&mut self, direction: SearchDirection) -> Result<(), AppError> {
        let query = get_window_text(self.find_edit)?;
        if query.is_empty() {
            self.show_find_bar = true;
            self.show_or_hide_find_bar();
            self.layout();
            unsafe {
                SetFocus(self.find_edit);
            }
            return Ok(());
        }

        self.sync_current_text()?;
        if self.show_search_results && !self.app.search_results().is_empty() {
            if let Some(index) = self.app.move_active_search_result(direction) {
                self.select_search_result(index)?;
            }
            return Ok(());
        }

        let Some(surface) = self.editor_text_surface() else {
            return Ok(());
        };
        let (selection_start, selection_end) = surface.selection();
        let start_units = match direction {
            SearchDirection::Forward => selection_end,
            SearchDirection::Backward => selection_start,
        };
        let current_document = self
            .app
            .current_document()
            .map(|document| (document.id(), document.content_snapshot()));
        let selection = current_document.and_then(|(document_id, content)| {
            find_text_rich_edit_offsets_cached(
                &mut self.search_offset_cache,
                document_id,
                content,
                &query,
                start_units as usize,
                direction,
            )
        });

        if let Some((start, end)) = selection {
            surface.set_selection(start, end);
        } else {
            message_box(self.hwnd, "No match.", "Find", MB_OK | MB_ICONINFORMATION);
        }

        Ok(())
    }

    fn find_all_results(&mut self) -> Result<(), AppError> {
        let query = get_window_text(self.find_edit)?;
        if query.is_empty() {
            self.show_find_bar = true;
            self.show_search_results = false;
            self.show_or_hide_find_bar();
            self.show_or_hide_search_results();
            self.layout();
            unsafe {
                SetFocus(self.find_edit);
            }
            return Ok(());
        }

        self.sync_current_text()?;
        self.app.update_search_results(&query)?;
        self.show_find_bar = true;
        self.show_search_results = true;
        self.populate_search_results()?;
        self.show_or_hide_find_bar();
        self.show_or_hide_search_results();
        self.layout();

        if self.app.search_results().is_empty() {
            message_box(
                self.hwnd,
                "No match.",
                "Results",
                MB_OK | MB_ICONINFORMATION,
            );
        } else {
            self.select_search_result(0)?;
        }
        Ok(())
    }

    fn populate_search_results(&self) -> Result<(), AppError> {
        unsafe {
            SendMessageW(self.search_results_list, LB_RESETCONTENT, 0, 0);
        }
        for result in self.app.search_results() {
            let label = format!(
                "{}:{}  {}",
                result.line,
                result.column,
                result.preview.trim()
            );
            listbox_add_string(self.search_results_list, &label)?;
        }
        Ok(())
    }

    fn select_search_result(&mut self, index: usize) -> Result<(), AppError> {
        self.app.set_active_search_result(index)?;
        let result = self
            .app
            .search_results()
            .get(index)
            .ok_or(AppError::InvalidState("Result not found."))?;
        let start_byte = result.range.start;
        let end_byte = result.range.end;
        let selection = self
            .app
            .current_document()
            .map(|document| (document.id(), document.content_snapshot()))
            .map(|(document_id, content)| {
                search_result_rich_edit_offsets_cached(
                    &mut self.search_offset_cache,
                    document_id,
                    content,
                    start_byte,
                    end_byte,
                )
            });
        unsafe {
            SendMessageW(self.search_results_list, LB_SETCURSEL, index, 0);
        }
        if let Some(surface) = self.editor_text_surface()
            && let Some((start, end)) = selection
        {
            surface.set_selection(start, end);
        }
        Ok(())
    }

    fn activate_selected_search_result(&mut self) -> Result<(), AppError> {
        let selected = unsafe { SendMessageW(self.search_results_list, LB_GETCURSEL, 0, 0) };
        if selected == LB_ERR {
            return Ok(());
        }
        self.select_search_result(selected as usize)
    }

    fn replace_current(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.ensure_current_writable()?;
        let query = get_window_text(self.find_edit)?;
        if query.is_empty() {
            return Ok(());
        }

        let replacement = get_window_text(self.replace_edit)?;
        self.sync_current_text()?;
        let Some(surface) = self.editor_text_surface() else {
            return Ok(());
        };
        let (selection_start, selection_end) = surface.selection();
        let selected_matches = match self.app.current_document() {
            Some(document) => {
                let text = document.content();
                let (start_byte, end_byte) = rich_edit_selection_byte_range(
                    text,
                    selection_start as usize,
                    selection_end as usize,
                )?;
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
            surface.replace_selection(&replacement)?;
            self.sync_after_text_surface_command()?;
            self.search(SearchDirection::Forward)
        } else {
            self.search(SearchDirection::Forward)
        }
    }

    fn replace_all(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.ensure_current_writable()?;
        let query = get_window_text(self.find_edit)?;
        if query.is_empty() {
            return Ok(());
        }

        let replacement = get_window_text(self.replace_edit)?;
        let Some(surface) = self.editor_text_surface() else {
            return Ok(());
        };
        let synced_text;
        let (text, source_char_count) = if self.edit_content_pending_sync {
            synced_text = surface.get_text_for_document_sync()?;
            (synced_text.text.as_str(), synced_text.char_count)
        } else if let Some(document) = self.app.current_document() {
            let text = document.content();
            validate_editor_document_text_size(text)?;
            (text, document.char_count())
        } else {
            synced_text = surface.get_text_for_document_sync()?;
            (synced_text.text.as_str(), synced_text.char_count)
        };
        if let Some(replaced) =
            replace_all_text_if_changed(text, source_char_count, &query, &replacement)?
        {
            let ReplaceAllText {
                text,
                byte_len,
                metrics,
            } = replaced;
            validate_editor_document_byte_len(byte_len)?;
            surface.set_text(&text)?;
            self.app
                .update_current_content_with_metrics(text, metrics)?;
            self.edit_content_pending_sync = false;
            self.invalidate_selection_metrics_cache();
            self.invalidate_search_offset_cache();
            self.app.clear_search_results();
            self.show_search_results = false;
            self.show_or_hide_search_results();
            self.refresh_current_tab_display()?;
            self.update_status()?;
            self.update_line_numbers()?;
        }

        Ok(())
    }

    fn layout(&self) {
        let mut rect = RECT::default();
        let ok = unsafe { GetClientRect(self.hwnd, &mut rect) };
        if ok == 0 {
            return;
        }

        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let ui = self.dpi_metrics.ui_scale();
        let tab_height = ui.px(TAB_HEIGHT);
        let find_bar_height = ui.px(FIND_BAR_HEIGHT);
        let search_results_height = ui.px(SEARCH_RESULTS_HEIGHT);
        let command_palette_height = ui.px(COMMAND_PALETTE_HEIGHT);
        let status_height = ui.px(STATUS_HEIGHT);
        let line_number_width = ui.px(LINE_NUMBER_WIDTH);
        let command_height = if self.show_command_palette {
            command_palette_height
        } else {
            0
        };
        let find_height = if self.show_find_bar {
            find_bar_height
        } else {
            0
        };
        let results_height = if self.show_find_bar && self.show_search_results {
            search_results_height
        } else {
            0
        };
        let editor_top = tab_height + command_height + find_height + results_height;
        let editor_height = (height - editor_top - status_height).max(0);
        let line_width = if self.show_line_numbers {
            line_number_width
        } else {
            0
        };

        move_window(self.tab, 0, 0, width, tab_height, true);
        self.layout_command_palette(width, tab_height, command_palette_height, ui);
        self.layout_find_bar(width, tab_height + command_height, ui);
        self.layout_search_results(
            width,
            tab_height + command_height + find_height,
            search_results_height,
            ui,
        );

        move_window(
            self.line_numbers,
            0,
            editor_top,
            line_width,
            editor_height,
            true,
        );
        show_window(self.line_numbers, self.show_line_numbers);
        move_window(
            self.edit,
            line_width,
            editor_top,
            (width - line_width).max(0),
            editor_height,
            true,
        );
        move_window(
            self.status,
            0,
            height - status_height,
            width,
            status_height,
            true,
        );
    }

    fn layout_find_bar(&self, width: i32, top: i32, ui: UiScale) {
        let visible = self.show_find_bar;
        let mut x = ui.px(8);
        let y = top + ui.px(5);
        let h = ui.px(24);

        show_window(self.find_label, visible);
        show_window(self.find_edit, visible);
        show_window(self.replace_label, visible);
        show_window(self.replace_edit, visible);
        show_window(self.find_next_button, visible);
        show_window(self.find_prev_button, visible);
        show_window(self.replace_button, visible);
        show_window(self.replace_all_button, visible);
        show_window(self.find_close_button, visible);
        show_window(self.find_all_button, visible);

        if !visible {
            return;
        }

        move_window(self.find_label, x, y, ui.px(42), h, true);
        x += ui.px(48);
        let edit_width = ((width - ui.px(560)) / 2).max(ui.px(120));
        move_window(self.find_edit, x, y, edit_width, h, true);
        x += edit_width + ui.px(8);
        move_window(self.replace_label, x, y, ui.px(58), h, true);
        x += ui.px(64);
        move_window(self.replace_edit, x, y, edit_width, h, true);
        x += edit_width + ui.px(8);
        move_window(self.find_next_button, x, y, ui.px(52), h, true);
        x += ui.px(56);
        move_window(self.find_prev_button, x, y, ui.px(52), h, true);
        x += ui.px(56);
        move_window(self.replace_button, x, y, ui.px(68), h, true);
        x += ui.px(72);
        move_window(self.replace_all_button, x, y, ui.px(42), h, true);
        x += ui.px(46);
        move_window(self.find_all_button, x, y, ui.px(70), h, true);
        move_window(
            self.find_close_button,
            width - ui.px(34),
            y,
            ui.px(26),
            h,
            true,
        );
    }

    fn layout_search_results(&self, width: i32, top: i32, results_height: i32, ui: UiScale) {
        let visible = self.show_find_bar && self.show_search_results;
        show_window(self.search_results_list, visible);
        if visible {
            move_window(
                self.search_results_list,
                ui.px(8),
                top,
                (width - ui.px(16)).max(0),
                (results_height - ui.px(6)).max(0),
                true,
            );
        }
    }

    fn layout_command_palette(&self, width: i32, top: i32, palette_height: i32, ui: UiScale) {
        let visible = self.show_command_palette;
        show_window(self.command_filter, visible);
        show_window(self.command_list, visible);
        if !visible {
            return;
        }
        move_window(
            self.command_filter,
            ui.px(8),
            top + ui.px(8),
            (width - ui.px(16)).max(0),
            ui.px(24),
            true,
        );
        move_window(
            self.command_list,
            ui.px(8),
            top + ui.px(38),
            (width - ui.px(16)).max(0),
            (palette_height - ui.px(46)).max(0),
            true,
        );
    }

    fn show_or_hide_find_bar(&self) {
        show_window(self.find_label, self.show_find_bar);
        show_window(self.find_edit, self.show_find_bar);
        show_window(self.replace_label, self.show_find_bar);
        show_window(self.replace_edit, self.show_find_bar);
        show_window(self.find_next_button, self.show_find_bar);
        show_window(self.find_prev_button, self.show_find_bar);
        show_window(self.replace_button, self.show_find_bar);
        show_window(self.replace_all_button, self.show_find_bar);
        show_window(self.find_close_button, self.show_find_bar);
        show_window(self.find_all_button, self.show_find_bar);
    }

    fn show_or_hide_search_results(&self) {
        show_window(
            self.search_results_list,
            self.show_find_bar && self.show_search_results,
        );
    }

    fn show_or_hide_command_palette(&self) {
        show_window(self.command_filter, self.show_command_palette);
        show_window(self.command_list, self.show_command_palette);
    }

    fn update_editor_surface_chrome_report_errors(&mut self) {
        self.with_refreshed_current_editor_status(|window, status| {
            window.apply_editor_surface_chrome_report_errors(status);
        });
    }

    fn update_editor_surface_chrome_if_changed_report_errors(&mut self) {
        self.with_refreshed_current_editor_status(|window, status| {
            if window.editor_chrome_snapshot_matches_current(status) {
                return;
            }
            window.apply_editor_surface_chrome_report_errors(status);
        });
    }

    fn apply_editor_surface_chrome_report_errors(&mut self, status: &CurrentEditorStatus) {
        let mut applied = true;
        if let Err(error) = self.update_status_from_current_editor_status(status) {
            applied = false;
            show_error(self.hwnd, &error);
        }
        if let Err(error) = self.update_line_numbers() {
            applied = false;
            show_error(self.hwnd, &error);
        }
        self.update_menu_checks_from_current_editor_status(status);
        if applied {
            self.capture_editor_chrome_snapshot(status);
        } else {
            self.editor_chrome_snapshot = None;
        }
    }

    fn update_editor_surface_chrome(&mut self) -> Result<(), AppError> {
        self.with_refreshed_current_editor_status(|window, status| {
            window.apply_editor_surface_chrome(status)
        })
    }

    fn apply_editor_surface_chrome(
        &mut self,
        status: &CurrentEditorStatus,
    ) -> Result<(), AppError> {
        self.update_status_from_current_editor_status(status)?;
        self.update_line_numbers()?;
        self.update_menu_checks_from_current_editor_status(status);
        self.capture_editor_chrome_snapshot(status);
        Ok(())
    }

    fn refresh_current_editor_status_into(&mut self, status: &mut CurrentEditorStatus) {
        self.refresh_current_editor_surface_state();
        self.app.current_editor_status_into(status);
    }

    fn with_refreshed_current_editor_status<R>(
        &mut self,
        update: impl FnOnce(&mut Self, &CurrentEditorStatus) -> R,
    ) -> R {
        let mut status = mem::take(&mut self.current_editor_status);
        self.refresh_current_editor_status_into(&mut status);
        let result = update(self, &status);
        self.current_editor_status = status;
        result
    }

    fn editor_chrome_snapshot_matches_current(&self, status: &CurrentEditorStatus) -> bool {
        if self.status_snapshot.is_none() {
            return false;
        }

        let candidate = self.editor_chrome_snapshot_candidate(status);
        if candidate.show_line_numbers
            && candidate.editor_surface_present
            && self.line_numbers_snapshot.is_none()
        {
            return false;
        }

        self.editor_chrome_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.matches(&candidate))
    }

    fn capture_editor_chrome_snapshot(&mut self, status: &CurrentEditorStatus) {
        let snapshot = {
            let candidate = self.editor_chrome_snapshot_candidate(status);
            EditorChromeSnapshot::from_candidate(&candidate)
        };
        self.editor_chrome_snapshot = Some(snapshot);
    }

    fn editor_chrome_snapshot_candidate<'a>(
        &'a self,
        status: &'a CurrentEditorStatus,
    ) -> EditorChromeSnapshotCandidate<'a> {
        let editor_surface_present = self.editor_text_surface().is_some();
        EditorChromeSnapshotCandidate {
            status,
            settings: self.app.settings(),
            show_line_numbers: self.show_line_numbers,
            show_command_palette: self.show_command_palette,
            editor_surface_present,
            dark_theme: self.dark_theme_active(),
        }
    }

    fn consume_text_change_editor_chrome_sync(&mut self, message: u32) -> bool {
        if !self.editor_chrome_synced_after_text_change {
            return false;
        }
        self.editor_chrome_synced_after_text_change = false;
        message == WM_KEYUP
    }

    fn update_status(&mut self) -> Result<(), AppError> {
        self.with_refreshed_current_editor_status(|window, status| {
            window.update_status_from_current_editor_status(status)
        })
    }

    fn update_status_from_current_editor_status(
        &mut self,
        status: &CurrentEditorStatus,
    ) -> Result<(), AppError> {
        let selection_start = status.selection_start_utf16;
        let selection_end = status.selection_end_utf16;
        let current_line = status.line;
        let column = status.column;
        let selected_chars = match self.app.current_document() {
            Some(document) => {
                let text = document.content();
                selected_char_count_cached(
                    &mut self.selection_metrics_cache,
                    document.id(),
                    text,
                    selection_start,
                    selection_end,
                )
            }
            None => {
                self.invalidate_selection_metrics_cache();
                0
            }
        };
        let document_id = status.document_id;
        let encoding = status.encoding.display_name();
        let line_ending = status.line_ending.display_name();
        let save_state = editor_save_state_label(status);
        let title = if status.title.is_empty() {
            APP_TITLE
        } else {
            status.title.as_str()
        };
        let char_count = status.char_count;
        let word_wrap = status.word_wrap;
        let dark_theme = self.dark_theme_active();
        let status_candidate = StatusSnapshotCandidate {
            document_id,
            selection_start,
            selection_end,
            current_line,
            column,
            selected_chars,
            char_count,
            encoding,
            line_ending,
            word_wrap,
            save_state,
            status_kind: status.status_kind,
            path: status.path.as_deref(),
            title,
            dark_theme,
        };

        if self
            .status_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.matches(&status_candidate))
        {
            return Ok(());
        }

        let encoding = encoding.to_string();
        let line_ending = line_ending.to_string();
        let state = editor_status_state_text(status);
        let save_state = save_state.to_string();
        let word_wrap_text = if word_wrap { "Wrap On" } else { "Wrap Off" }.to_string();
        let title = title.to_string();

        let parts = [
            (140, format!("Line {}, Col {}", current_line, column)),
            (260, format!("Chars {}", char_count)),
            (390, format!("Selected {}", selected_chars)),
            (500, encoding.clone()),
            (610, line_ending.clone()),
            (720, word_wrap_text),
            (850, save_state.clone()),
            (-1, state.clone()),
        ];

        let edges = [140, 260, 390, 500, 610, 720, 850, -1];
        unsafe {
            SendMessageW(
                self.status,
                SB_SETPARTS,
                edges.len(),
                edges.as_ptr() as LPARAM,
            );
        }

        self.status_parts = parts.iter().map(|(_, value)| value.clone()).collect();
        for (index, value) in self.status_parts.iter().enumerate() {
            if dark_theme {
                unsafe {
                    SendMessageW(
                        self.status,
                        SB_SETTEXTW,
                        index | SBT_OWNERDRAW as usize,
                        index as LPARAM,
                    );
                }
            } else {
                let wide = wide_null(value);
                unsafe {
                    SendMessageW(self.status, SB_SETTEXTW, index, wide.as_ptr() as LPARAM);
                }
            }
        }

        let window_title = format!("{title} - {APP_TITLE}");
        set_window_text(self.hwnd, &window_title)?;
        self.status_snapshot = Some(StatusSnapshot {
            document_id,
            selection_start,
            selection_end,
            current_line,
            column,
            selected_chars,
            char_count,
            encoding,
            line_ending,
            word_wrap,
            save_state,
            status_kind: status.status_kind,
            path: status.path.clone(),
            title,
            dark_theme,
        });
        Ok(())
    }

    fn update_line_numbers(&mut self) -> Result<(), AppError> {
        if !self.show_line_numbers {
            self.line_numbers_snapshot = None;
            return Ok(());
        }

        let Some(surface) = self.editor_text_surface() else {
            self.line_numbers_snapshot = None;
            return Ok(());
        };
        let first_line = surface.first_visible_line();
        let line_count = surface.line_count();
        let visible_count = 200usize.min(line_count.saturating_sub(first_line).saturating_add(1));
        let snapshot = LineNumbersSnapshot {
            first_line,
            visible_count,
        };
        if self.line_numbers_snapshot == Some(snapshot) {
            return Ok(());
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
        let mut labels = String::with_capacity(visible_count.saturating_mul(label_digits + 2));

        for line in first_line..first_line.saturating_add(visible_count) {
            if line >= line_count {
                break;
            }
            let _ = write!(&mut labels, "{}\r\n", line + 1);
        }

        set_window_text(self.line_numbers, &labels)?;
        unsafe {
            InvalidateRect(self.line_numbers, null(), 1);
        }
        self.line_numbers_snapshot = Some(snapshot);
        Ok(())
    }

    fn show_startup_warnings(&mut self) {
        if self.startup_warnings.is_empty() {
            return;
        }

        let message = self.startup_warnings.join("\n");
        self.startup_warnings.clear();
        message_box(self.hwnd, &message, "Startup", MB_OK | MB_ICONWARNING);
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
            let answer = message_box(self.hwnd, &message, "Large File", MB_YESNO | MB_ICONWARNING);
            if answer == IDYES {
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
        let Some(pending) = &self.pending_save else {
            return false;
        };
        self.app
            .current_document()
            .is_some_and(|document| document.id() == pending.document_id)
    }

    fn invalidate_selection_metrics_cache(&mut self) {
        self.selection_metrics_cache = None;
        self.status_snapshot = None;
        self.editor_chrome_snapshot = None;
    }

    fn invalidate_search_offset_cache(&mut self) {
        self.search_offset_cache = None;
    }

    fn apply_current_read_only(&self) -> Result<(), AppError> {
        let Some(surface) = self.editor_text_surface() else {
            return Ok(());
        };
        let read_only = self
            .app
            .current_document()
            .is_some_and(|document| document.is_read_only())
            || self.app.settings().show_whitespace
            || self.current_document_is_saving();
        surface.set_readonly(read_only)?;
        let text_limit = if read_only {
            RICH_EDIT_SURFACE_TEXT_LIMIT
        } else {
            RICH_EDIT_EDITABLE_TEXT_LIMIT
        };
        surface.set_text_limit(text_limit);
        Ok(())
    }

    fn move_current_tab_left(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        if self.app.move_current_tab_left() {
            self.refresh_tabs()?;
            self.load_current_document_into_edit()?;
            self.persist_recent_files_report_errors();
        }
        Ok(())
    }

    fn move_current_tab_right(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        if self.app.move_current_tab_right() {
            self.refresh_tabs()?;
            self.load_current_document_into_edit()?;
            self.persist_recent_files_report_errors();
        }
        Ok(())
    }

    fn toggle_visible_whitespace(&mut self) -> Result<(), AppError> {
        self.ensure_no_pending_save()?;
        self.sync_current_text()?;
        let enable_whitespace = !self.app.settings().show_whitespace;
        if enable_whitespace
            && let Some(byte_len) = self
                .app
                .current_document()
                .map(|document| document.content().len())
                .filter(|byte_len| !can_render_visible_whitespace_bytes(*byte_len))
        {
            show_visible_whitespace_size_limit(self.hwnd, byte_len);
            return Ok(());
        }

        let mut settings = self.app.settings().clone();
        settings.show_whitespace = enable_whitespace;
        self.app.set_settings(settings);
        self.load_current_document_into_edit()?;
        self.update_status()?;
        self.update_menu_checks();
        self.persist_settings_report_errors();
        Ok(())
    }

    fn toggle_command_palette(&mut self) -> Result<(), AppError> {
        self.show_command_palette = !self.show_command_palette;
        if self.show_command_palette {
            self.update_command_palette_filter()?;
            unsafe {
                SetFocus(self.command_filter);
            }
        }
        self.show_or_hide_command_palette();
        self.layout();
        self.update_menu_checks();
        Ok(())
    }

    fn update_command_palette_filter(&mut self) -> Result<(), AppError> {
        if self.command_list.is_null() {
            return Ok(());
        }
        let filter = if self.command_filter.is_null() {
            String::new()
        } else {
            get_window_text(self.command_filter)?
        };
        let filter = filter.to_ascii_lowercase();
        self.filtered_command_ids.clear();
        unsafe {
            SendMessageW(self.command_list, LB_RESETCONTENT, 0, 0);
        }

        for command in self.command_items.clone() {
            let label = command_label(command);
            if !filter.is_empty() && !label.to_ascii_lowercase().contains(&filter) {
                continue;
            }
            record_command_palette_id_after_listbox_add(
                &mut self.filtered_command_ids,
                command.id,
                listbox_add_string(self.command_list, &label),
            )?;
        }

        if !self.filtered_command_ids.is_empty() {
            unsafe {
                SendMessageW(self.command_list, LB_SETCURSEL, 0, 0);
            }
        }
        Ok(())
    }

    fn activate_selected_command(&mut self) -> Result<(), AppError> {
        let selected = unsafe { SendMessageW(self.command_list, LB_GETCURSEL, 0, 0) };
        if selected == LB_ERR {
            return Ok(());
        }
        let Some(command_id) = self.filtered_command_ids.get(selected as usize).copied() else {
            return Ok(());
        };
        self.show_command_palette = false;
        self.show_or_hide_command_palette();
        self.layout();
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
            EditorCommandId::Undo => self.text_surface_command(true, EditorTextSurface::undo),
            EditorCommandId::Redo => self.text_surface_command(true, EditorTextSurface::redo),
            EditorCommandId::Cut => self.text_surface_command(true, EditorTextSurface::cut),
            EditorCommandId::Copy => self.text_surface_command(false, EditorTextSurface::copy),
            EditorCommandId::Paste => self.paste_plain_text(),
            EditorCommandId::SelectAll => {
                self.select_all_text();
                Ok(())
            }
            EditorCommandId::Find => {
                self.show_find_bar = true;
                self.show_or_hide_find_bar();
                self.layout();
                unsafe {
                    SetFocus(self.find_edit);
                }
                Ok(())
            }
            EditorCommandId::Replace => {
                self.show_find_bar = true;
                self.show_or_hide_find_bar();
                self.layout();
                unsafe {
                    SetFocus(self.replace_edit);
                }
                Ok(())
            }
            EditorCommandId::FindAll => self.find_all_results(),
            EditorCommandId::FindNext => self.search(SearchDirection::Forward),
            EditorCommandId::FindPrevious => self.search(SearchDirection::Backward),
            EditorCommandId::CommandPalette => self.toggle_command_palette(),
            EditorCommandId::ToggleLineNumbers => {
                self.show_line_numbers = !self.show_line_numbers;
                self.layout();
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

    fn text_surface_command(
        &mut self,
        mutates_text: bool,
        command: impl FnOnce(EditorTextSurface),
    ) -> Result<(), AppError> {
        let Some(surface) = self.editor_text_surface() else {
            return Ok(());
        };
        if mutates_text {
            self.ensure_current_writable()?;
        }
        command(surface);
        if mutates_text {
            self.sync_after_text_surface_command()
        } else {
            self.update_status()
        }
    }

    fn paste_plain_text(&mut self) -> Result<(), AppError> {
        let Some(surface) = self.editor_text_surface() else {
            return Ok(());
        };
        self.ensure_current_writable()?;
        surface.paste_plain_text();
        self.sync_after_text_surface_command()
    }

    fn sync_after_text_surface_command(&mut self) -> Result<(), AppError> {
        self.edit_content_pending_sync = true;
        if let Err(error) = self.sync_current_text() {
            self.rollback_current_editor_surface_after_failed_sync()?;
            return Err(error);
        }
        self.refresh_current_tab_display()?;
        self.update_status()
            .and_then(|()| self.update_line_numbers())?;
        self.update_menu_checks();
        Ok(())
    }

    fn rollback_current_editor_surface_after_failed_sync(&mut self) -> Result<(), AppError> {
        self.load_current_document_into_edit()
    }

    fn select_all_text(&self) {
        if let Some(surface) = self.editor_text_surface() {
            surface.select_all();
        }
    }

    fn choose_font(&mut self) -> Result<(), AppError> {
        let Some(settings) = choose_font_dialog(self.hwnd, self.app.settings())? else {
            return Ok(());
        };
        self.apply_settings(settings)
    }

    fn set_tab_size(&mut self, tab_size: u8) -> Result<(), AppError> {
        let mut settings = self.app.settings().clone();
        settings.tab_size = tab_size;
        self.apply_settings(settings)
    }

    fn toggle_word_wrap(&mut self) -> Result<(), AppError> {
        let mut settings = self.app.settings().clone();
        settings.word_wrap = !settings.word_wrap;
        self.apply_settings(settings)
    }

    fn set_theme(&mut self, theme: ThemeMode) -> Result<(), AppError> {
        let mut settings = self.app.settings().clone();
        settings.theme = theme;
        self.apply_settings(settings)
    }

    fn configure_shortcut(
        &mut self,
        command: EditorCommandId,
        action: ShortcutMenuAction,
    ) -> Result<(), AppError> {
        let shortcut = match action {
            ShortcutMenuAction::Capture => {
                let Some(captured) = capture_shortcut_dialog(
                    self.hwnd,
                    command.shortcut_title().unwrap_or("Command"),
                )?
                else {
                    return Ok(());
                };
                Some(captured)
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
            let response = message_box(
                self.hwnd,
                &format!(
                    "{} is used by {}.\n\nUse it here?",
                    shortcut.display_name(),
                    existing_title
                ),
                "Shortcut",
                MB_YESNO | MB_ICONWARNING,
            );
            if response != IDYES {
                return Ok(());
            }
            settings
                .shortcuts
                .clear_matching_shortcut(shortcut, command);
        }
        settings.shortcuts.set_shortcut(command, shortcut);
        self.apply_settings(settings)
    }

    fn apply_settings(&mut self, settings: EditorSettings) -> Result<(), AppError> {
        self.app.set_settings(settings);
        self.recreate_font()?;
        self.apply_font_to_controls();
        self.apply_tab_stops();
        self.apply_word_wrap_style();
        self.create_menu()?;
        self.apply_theme();
        self.layout();
        self.update_status()?;
        self.persist_settings_report_errors();
        Ok(())
    }

    fn recreate_font(&mut self) -> Result<(), AppError> {
        self.release_owned_font();
        let settings = self.app.settings();
        let height = self
            .dpi_metrics
            .ui_scale()
            .font_height(settings.font_size_pt);
        let face = wide_null(&settings.font_name);
        let font =
            unsafe { CreateFontW(height, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, face.as_ptr()) };

        if font.is_null() {
            let fixed_object = unsafe { GetStockObject(ANSI_FIXED_FONT) };
            if fixed_object.is_null() {
                return Err(last_win32_error("load fixed font"));
            }
            self.fixed_font = fixed_object as HFONT;
            self.owns_font = false;
        } else {
            self.fixed_font = font;
            self.owns_font = true;
        }
        Ok(())
    }

    fn release_owned_font(&mut self) {
        if self.owns_font && !self.fixed_font.is_null() {
            unsafe {
                DeleteObject(self.fixed_font as _);
            }
            self.fixed_font = null_mut();
            self.owns_font = false;
        }
    }

    fn apply_tab_stops(&self) {
        if let Some(surface) = self.editor_text_surface() {
            let tab_stop = i32::from(self.app.settings().tab_size) * 4;
            surface.set_tab_stops(tab_stop);
        }
    }

    fn apply_word_wrap_style(&self) {
        if let Some(surface) = self.editor_text_surface() {
            surface.set_word_wrap(self.app.settings().word_wrap);
        }
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
        let pending = self.pending_persistence.take_pending();
        if !pending.has_pending() {
            return;
        }

        if pending.settings && !self.save_settings_now_report_errors() {
            self.pending_persistence.request_settings();
        }
        if pending.recent_files && !self.save_recent_files_now_report_errors() {
            self.pending_persistence.request_recent_files();
        }
    }

    fn save_settings_now_report_errors(&mut self) -> bool {
        if let Some(store) = &self.store
            && let Err(error) = store.save_settings(self.app.settings())
        {
            self.last_persist_error = Some(error.user_message());
            return false;
        }
        true
    }

    fn save_recent_files_now_report_errors(&mut self) -> bool {
        if let Some(store) = &self.store
            && let Err(error) = store.save_recent_files(self.app.recent_files())
        {
            self.last_persist_error = Some(error.user_message());
            return false;
        }
        true
    }

    fn handle_drop_files_report_errors(&mut self, drop: HDROP) {
        if let Err(error) = self.handle_drop_files(drop) {
            show_error(self.hwnd, &error);
        }
    }

    fn handle_drop_files(&mut self, drop: HDROP) -> Result<(), AppError> {
        let mut drop_files = DropFilesGuard::new(drop);
        self.ensure_no_pending_save()?;
        let paths = dropped_paths(drop_files.handle());
        drop_files.finish();
        for path in paths {
            self.open_path_as_new_tab(path, None)?;
        }
        Ok(())
    }

    fn update_menu_checks(&mut self) {
        self.with_refreshed_current_editor_status(|window, status| {
            window.update_menu_checks_from_current_editor_status(status);
        });
    }

    fn update_menu_checks_from_current_editor_status(&mut self, status: &CurrentEditorStatus) {
        let menu = unsafe { GetMenu(self.hwnd) };
        if menu.is_null() {
            return;
        }

        check_menu(menu, ID_VIEW_LINE_NUMBERS, self.show_line_numbers);
        check_menu(
            menu,
            ID_VIEW_WHITESPACE,
            self.app.settings().show_whitespace,
        );
        check_menu(menu, ID_VIEW_COMMAND_PALETTE, self.show_command_palette);
        check_menu(
            menu,
            ID_LINE_ENDING_CRLF,
            status.document_id.is_some() && status.line_ending == LineEnding::Crlf,
        );
        check_menu(
            menu,
            ID_LINE_ENDING_LF,
            status.document_id.is_some() && status.line_ending == LineEnding::Lf,
        );
        check_menu(
            menu,
            ID_LINE_ENDING_CR,
            status.document_id.is_some() && status.line_ending == LineEnding::Cr,
        );

        let settings = self.app.settings();
        let editor_available = status.document_id.is_some() && self.editor_text_surface().is_some();
        enable_menu_item(menu, ID_FILE_SAVE, status.can_save);
        enable_menu_item(menu, ID_FILE_SAVE_AS, status.can_save_as);
        enable_menu_item(menu, ID_EDIT_UNDO, status.can_undo);
        enable_menu_item(menu, ID_EDIT_REDO, status.can_redo);
        enable_menu_item(menu, ID_EDIT_CUT, status.can_edit);
        enable_menu_item(menu, ID_EDIT_PASTE, status.can_edit);
        enable_menu_item(menu, ID_EDIT_COPY, editor_available);
        enable_menu_item(menu, ID_EDIT_SELECT_ALL, editor_available);
        enable_menu_item(menu, ID_ENCODING_CONVERT, status.can_edit);
        enable_menu_item(menu, ID_LINE_ENDING_CRLF, status.can_edit);
        enable_menu_item(menu, ID_LINE_ENDING_LF, status.can_edit);
        enable_menu_item(menu, ID_LINE_ENDING_CR, status.can_edit);
        check_menu(menu, ID_SETTINGS_TAB_SIZE_2, settings.tab_size == 2);
        check_menu(menu, ID_SETTINGS_TAB_SIZE_4, settings.tab_size == 4);
        check_menu(menu, ID_SETTINGS_TAB_SIZE_8, settings.tab_size == 8);
        check_menu(menu, ID_SETTINGS_WORD_WRAP, settings.word_wrap);
        for theme in ThemeMode::options() {
            check_menu(menu, theme_command_id(*theme), settings.theme == *theme);
        }
        for (index, command) in EditorCommandId::SHORTCUT_COMMANDS
            .iter()
            .copied()
            .enumerate()
        {
            check_menu(
                menu,
                shortcut_settings_command_id(index, ShortcutMenuAction::UseDefault),
                settings.shortcuts.shortcut_for(command) == command.default_shortcut(),
            );
            check_menu(
                menu,
                shortcut_settings_command_id(index, ShortcutMenuAction::Disable),
                settings.shortcuts.shortcut_for(command).is_none(),
            );
        }
    }
}

struct MainMenuSnapshot<'a> {
    settings: &'a EditorSettings,
    recent_files: &'a [PathBuf],
}

impl<'a> MainMenuSnapshot<'a> {
    fn new(settings: &'a EditorSettings, recent_files: &'a [PathBuf]) -> Self {
        Self {
            settings,
            recent_files,
        }
    }
}

struct MainMenuBuilder<'a> {
    snapshot: MainMenuSnapshot<'a>,
}

impl<'a> MainMenuBuilder<'a> {
    fn new(snapshot: MainMenuSnapshot<'a>) -> Self {
        Self { snapshot }
    }

    fn build(self) -> Result<MenuGuard, AppError> {
        let menu_guard = MenuGuard::create_menu("create menu")?;
        let menu = menu_guard.handle();

        let mut file = self.build_file_menu()?;
        Self::append_popup_and_release(menu, &mut file, "File")?;
        let mut edit = self.build_edit_menu()?;
        Self::append_popup_and_release(menu, &mut edit, "Edit")?;
        let mut search = self.build_search_menu()?;
        Self::append_popup_and_release(menu, &mut search, "Find")?;
        let mut view = self.build_view_menu()?;
        Self::append_popup_and_release(menu, &mut view, "View")?;
        let mut tabs = self.build_tabs_menu()?;
        Self::append_popup_and_release(menu, &mut tabs, "Tabs")?;
        let mut document = self.build_document_menu()?;
        Self::append_popup_and_release(menu, &mut document, "Text")?;
        let mut settings = self.build_settings_menu()?;
        Self::append_popup_and_release(menu, &mut settings, "Settings")?;
        let mut help = self.build_help_menu()?;
        Self::append_popup_and_release(menu, &mut help, "Help")?;

        Ok(menu_guard)
    }

    fn build_file_menu(&self) -> Result<MenuGuard, AppError> {
        let guard = MenuGuard::create_popup("create submenu")?;
        let menu = guard.handle();
        let mut recent_guard = MenuGuard::create_popup("create submenu")?;
        let recent_menu = recent_guard.handle();

        append_item(
            menu,
            ID_FILE_NEW,
            &self.command_label("New", EditorCommandId::NewFile),
        )?;
        append_item(
            menu,
            ID_FILE_OPEN,
            &self.command_label("Open...", EditorCommandId::OpenFile),
        )?;
        self.append_recent_files(recent_menu)?;
        append_popup(menu, recent_menu, "Recent")?;
        recent_guard.release();
        append_separator(menu)?;
        append_item(
            menu,
            ID_FILE_SAVE,
            &self.command_label("Save", EditorCommandId::Save),
        )?;
        append_item(
            menu,
            ID_FILE_SAVE_AS,
            &self.command_label("Save As...", EditorCommandId::SaveAs),
        )?;
        append_separator(menu)?;
        append_item(
            menu,
            ID_FILE_CLOSE_TAB,
            &self.command_label("Close", EditorCommandId::CloseTab),
        )?;
        append_item(menu, ID_FILE_CLOSE_ALL_TABS, "Close All")?;
        append_separator(menu)?;
        append_item(menu, ID_FILE_EXIT, "Exit")?;

        Ok(guard)
    }

    fn build_edit_menu(&self) -> Result<MenuGuard, AppError> {
        let guard = MenuGuard::create_popup("create submenu")?;
        let menu = guard.handle();

        append_item(
            menu,
            ID_EDIT_UNDO,
            &self.command_label("Undo", EditorCommandId::Undo),
        )?;
        append_item(
            menu,
            ID_EDIT_REDO,
            &self.command_label("Redo", EditorCommandId::Redo),
        )?;
        append_separator(menu)?;
        append_item(
            menu,
            ID_EDIT_CUT,
            &self.command_label("Cut", EditorCommandId::Cut),
        )?;
        append_item(
            menu,
            ID_EDIT_COPY,
            &self.command_label("Copy", EditorCommandId::Copy),
        )?;
        append_item(
            menu,
            ID_EDIT_PASTE,
            &self.command_label("Paste", EditorCommandId::Paste),
        )?;
        append_item(
            menu,
            ID_EDIT_SELECT_ALL,
            &self.command_label("Select All", EditorCommandId::SelectAll),
        )?;

        Ok(guard)
    }

    fn build_search_menu(&self) -> Result<MenuGuard, AppError> {
        let guard = MenuGuard::create_popup("create submenu")?;
        let menu = guard.handle();

        append_item(
            menu,
            ID_EDIT_FIND,
            &self.command_label("Find...", EditorCommandId::Find),
        )?;
        append_item(
            menu,
            ID_EDIT_REPLACE,
            &self.command_label("Replace...", EditorCommandId::Replace),
        )?;
        append_separator(menu)?;
        append_item(
            menu,
            ID_EDIT_FIND_NEXT,
            &self.command_label("Find Next", EditorCommandId::FindNext),
        )?;
        append_item(
            menu,
            ID_EDIT_FIND_PREVIOUS,
            &self.command_label("Find Prev", EditorCommandId::FindPrevious),
        )?;
        append_item(
            menu,
            ID_EDIT_FIND_ALL,
            &self.command_label("Find All", EditorCommandId::FindAll),
        )?;

        Ok(guard)
    }

    fn build_view_menu(&self) -> Result<MenuGuard, AppError> {
        let guard = MenuGuard::create_popup("create submenu")?;
        let menu = guard.handle();
        let mut theme_guard = MenuGuard::create_popup("create submenu")?;
        let theme_menu = theme_guard.handle();

        append_item(
            menu,
            ID_VIEW_COMMAND_PALETTE,
            &self.command_label("Commands...", EditorCommandId::CommandPalette),
        )?;
        append_separator(menu)?;
        append_item(menu, ID_VIEW_LINE_NUMBERS, "Line Numbers")?;
        append_item(menu, ID_VIEW_WHITESPACE, "Marks")?;
        append_item(
            menu,
            ID_SETTINGS_WORD_WRAP,
            &self.command_label("Word Wrap", EditorCommandId::ToggleWordWrap),
        )?;
        append_separator(menu)?;
        for theme in ThemeMode::options() {
            append_item(theme_menu, theme_command_id(*theme), theme.display_name())?;
        }
        append_popup(menu, theme_menu, "Theme")?;
        theme_guard.release();

        Ok(guard)
    }

    fn build_tabs_menu(&self) -> Result<MenuGuard, AppError> {
        let guard = MenuGuard::create_popup("create submenu")?;
        let menu = guard.handle();

        append_item(menu, ID_TAB_MOVE_LEFT, "Move Left")?;
        append_item(menu, ID_TAB_MOVE_RIGHT, "Move Right")?;
        append_separator(menu)?;
        append_item(
            menu,
            ID_FILE_CLOSE_TAB,
            &self.command_label("Close", EditorCommandId::CloseTab),
        )?;
        append_item(menu, ID_FILE_CLOSE_OTHER_TABS, "Close Others")?;
        append_item(menu, ID_FILE_CLOSE_ALL_TABS, "Close All")?;

        Ok(guard)
    }

    fn build_document_menu(&self) -> Result<MenuGuard, AppError> {
        let guard = MenuGuard::create_popup("create submenu")?;
        let menu = guard.handle();
        let mut line_ending_guard = MenuGuard::create_popup("create submenu")?;
        let line_ending = line_ending_guard.handle();

        append_item(menu, ID_ENCODING_REOPEN, "Reopen Encoding...")?;
        append_item(menu, ID_ENCODING_CONVERT, "Change Encoding...")?;
        append_separator(menu)?;
        append_item(line_ending, ID_LINE_ENDING_CRLF, "CRLF")?;
        append_item(line_ending, ID_LINE_ENDING_LF, "LF")?;
        append_item(line_ending, ID_LINE_ENDING_CR, "CR")?;
        append_popup(menu, line_ending, "Line Ends")?;
        line_ending_guard.release();

        Ok(guard)
    }

    fn build_settings_menu(&self) -> Result<MenuGuard, AppError> {
        let guard = MenuGuard::create_popup("create submenu")?;
        let menu = guard.handle();
        let mut tab_size_guard = MenuGuard::create_popup("create submenu")?;
        let tab_size_menu = tab_size_guard.handle();
        let mut shortcut_guard = MenuGuard::create_popup("create submenu")?;
        let shortcut_menu = shortcut_guard.handle();

        append_item(menu, ID_SETTINGS_CHOOSE_FONT, "Font...")?;
        append_item(tab_size_menu, ID_SETTINGS_TAB_SIZE_2, "2 spaces")?;
        append_item(tab_size_menu, ID_SETTINGS_TAB_SIZE_4, "4 spaces")?;
        append_item(tab_size_menu, ID_SETTINGS_TAB_SIZE_8, "8 spaces")?;
        append_shortcut_settings_items(shortcut_menu, &self.snapshot.settings.shortcuts)?;
        append_popup(menu, tab_size_menu, "Tab Size")?;
        tab_size_guard.release();
        append_popup(menu, shortcut_menu, "Shortcuts")?;
        shortcut_guard.release();

        Ok(guard)
    }

    fn build_help_menu(&self) -> Result<MenuGuard, AppError> {
        let guard = MenuGuard::create_popup("create submenu")?;
        append_item(guard.handle(), ID_HELP_ABOUT, "About")?;
        Ok(guard)
    }

    fn append_recent_files(&self, menu: HMENU) -> Result<(), AppError> {
        if self.snapshot.recent_files.is_empty() {
            append_disabled_item(menu, "(None)")?;
            return Ok(());
        }

        for (index, path) in self.snapshot.recent_files.iter().enumerate() {
            if index >= 10 {
                break;
            }
            let label = format!("{} {}", index + 1, path.display());
            append_item(menu, ID_FILE_RECENT_BASE + index as u16, &label)?;
        }
        Ok(())
    }

    fn command_label(&self, title: &str, command: EditorCommandId) -> String {
        command_menu_label(
            title,
            self.snapshot.settings.shortcuts.shortcut_for(command),
        )
    }

    fn append_popup_and_release(
        menu: HMENU,
        submenu: &mut MenuGuard,
        label: &str,
    ) -> Result<(), AppError> {
        append_popup(menu, submenu.handle(), label)?;
        submenu.release();
        Ok(())
    }
}

fn install_main_menu(hwnd: HWND, menu_guard: &mut MenuGuard) -> Result<(), AppError> {
    let menu = menu_guard.handle();
    let previous_menu = unsafe { GetMenu(hwnd) };
    let ok = unsafe { SetMenu(hwnd, menu) };
    if ok == 0 {
        return Err(last_win32_error("set menu"));
    }
    menu_guard.release();
    if !previous_menu.is_null() && previous_menu != menu {
        unsafe {
            DestroyMenu(previous_menu);
        }
    }

    Ok(())
}

extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        enable_non_client_dpi_scaling(hwnd);

        let create = lparam as *const CREATESTRUCTW;
        if !create.is_null() {
            let create_context =
                unsafe { (*create).lpCreateParams as *mut MainWindowCreateContext };
            if !create_context.is_null() {
                let state = unsafe { (*create_context).window_ptr };
                if !state.is_null() {
                    unsafe {
                        (*state).hwnd = hwnd;
                        (*state).create_context = create_context;
                        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
                        (*create_context).owned_by_window = true;
                    }
                }
            }
        }
    }

    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MainWindow };
    if !state.is_null() {
        let result = unsafe { (*state).handle_message(hwnd, message, wparam, lparam) };
        if message == WM_NCDESTROY {
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(state));
            }
        }
        return result;
    }

    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn shortcut_from_key_message(message: &MSG) -> Option<KeyboardShortcut> {
    shortcut_from_virtual_key(message.wParam as u32)
}

fn shortcut_from_virtual_key(virtual_key: u32) -> Option<KeyboardShortcut> {
    let key = shortcut_key_from_virtual_key(virtual_key)?;
    let shortcut = KeyboardShortcut {
        ctrl: key_state_down(VK_CONTROL_CODE),
        alt: key_state_down(VK_MENU_CODE),
        shift: key_state_down(VK_SHIFT_CODE),
        key,
    };
    shortcut.is_safe_for_text_editor().then_some(shortcut)
}

fn shortcut_key_from_virtual_key(virtual_key: u32) -> Option<ShortcutKey> {
    match virtual_key {
        0x30..=0x39 | 0x41..=0x5a => char::from_u32(virtual_key).map(ShortcutKey::Character),
        VK_F1_CODE..=VK_F24_CODE => {
            Some(ShortcutKey::Function((virtual_key - VK_F1_CODE + 1) as u8))
        }
        _ => None,
    }
}

fn key_state_down(virtual_key: i32) -> bool {
    unsafe { GetKeyState(virtual_key) < 0 }
}

fn open_file_dialog(owner: HWND) -> Result<Option<PathBuf>, AppError> {
    file_dialog(owner, true)
}

fn save_file_dialog(owner: HWND) -> Result<Option<PathBuf>, AppError> {
    file_dialog(owner, false)
}

fn file_dialog(owner: HWND, open: bool) -> Result<Option<PathBuf>, AppError> {
    let mut buffer = vec![0u16; 32768];
    let mut filter = wide_null("Text (*.txt)\0*.txt\0All (*.*)\0*.*\0");
    let mut title = wide_null(if open { "Open" } else { "Save As" });

    let mut ofn = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: filter.as_mut_ptr(),
        lpstrFile: buffer.as_mut_ptr(),
        nMaxFile: buffer.len() as u32,
        lpstrTitle: title.as_mut_ptr(),
        Flags: OFN_EXPLORER
            | OFN_PATHMUSTEXIST
            | if open {
                OFN_FILEMUSTEXIST | OFN_HIDEREADONLY
            } else {
                OFN_OVERWRITEPROMPT
            },
        ..unsafe { MaybeUninit::<OPENFILENAMEW>::zeroed().assume_init() }
    };

    let ok: windows_sys::core::BOOL = with_centered_dialog(owner, || {
        if open {
            unsafe { GetOpenFileNameW(&mut ofn) }
        } else {
            unsafe { GetSaveFileNameW(&mut ofn) }
        }
    });

    if ok == 0 {
        let code = unsafe { CommDlgExtendedError() };
        if code == 0 {
            return Ok(None);
        }
        return Err(AppError::dialog(code, "file dialog"));
    }

    let path_len = buffer
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(buffer.len());
    let os = OsString::from_wide(&buffer[..path_len]);
    Ok(Some(PathBuf::from(os)))
}

fn choose_encoding_dialog(
    owner: HWND,
    title: &str,
    instruction: &str,
    content: &str,
    current: TextEncoding,
) -> Result<Option<TextEncoding>, AppError> {
    let window = create_encoding_dialog_window(owner, title)?;
    let _window_guard = WindowDestroyGuard::new(window);
    let _instruction_label = create_dialog_control(
        window,
        DialogControlSpec {
            class_name: "STATIC",
            text: instruction,
            bounds: (16, 14, 456, 24),
            style: 0,
            ex_style: 0,
            id: 0,
        },
    )?;
    let _content_label = create_dialog_control(
        window,
        DialogControlSpec {
            class_name: "STATIC",
            text: content,
            bounds: (16, 42, 456, 40),
            style: 0,
            ex_style: 0,
            id: 0,
        },
    )?;
    let combo = create_dialog_control(
        window,
        DialogControlSpec {
            class_name: "COMBOBOX",
            text: "",
            bounds: (16, 90, 456, 220),
            style: CBS_DROPDOWN | CBS_HASSTRINGS | CBS_AUTOHSCROLL | WS_VSCROLL | WS_TABSTOP,
            ex_style: WS_EX_CLIENTEDGE,
            id: ID_ENCODING_COMBO,
        },
    )?;
    let status_label = create_dialog_control(
        window,
        DialogControlSpec {
            class_name: "STATIC",
            text: "",
            bounds: (16, 126, 456, 24),
            style: SS_CENTERIMAGE,
            ex_style: 0,
            id: ID_ENCODING_STATUS,
        },
    )?;
    let ok_button = create_dialog_control(
        window,
        DialogControlSpec {
            class_name: "BUTTON",
            text: "OK",
            bounds: (284, 164, 88, 28),
            style: BS_PUSHBUTTON as u32 | WS_TABSTOP,
            ex_style: 0,
            id: ID_ENCODING_OK,
        },
    )?;
    let cancel_button = create_dialog_control(
        window,
        DialogControlSpec {
            class_name: "BUTTON",
            text: "Cancel",
            bounds: (384, 164, 88, 28),
            style: BS_PUSHBUTTON as u32 | WS_TABSTOP,
            ex_style: 0,
            id: ID_ENCODING_CANCEL,
        },
    )?;

    fill_encoding_combo(combo, current)?;
    update_encoding_validation_status(combo, status_label)?;
    center_window_over_owner(window, owner);

    let _modal_guard = CustomModalDialogGuard::install(owner);
    unsafe {
        ShowWindow(window, SW_SHOW);
        SetFocus(combo);
    }

    let mut selected = None;
    let mut message = unsafe { MaybeUninit::<MSG>::zeroed().assume_init() };
    while unsafe { IsWindow(window) != 0 } {
        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if result == -1 {
            unsafe {
                DestroyWindow(window);
            }
            return Err(last_win32_error("get encoding dialog message"));
        }
        if result == 0 {
            unsafe {
                DestroyWindow(window);
                PostQuitMessage(0);
            }
            break;
        }

        if message.hwnd == window
            && (message.message == WM_CLOSE
                || (message.message == WM_SYSCOMMAND
                    && (message.wParam & 0xfff0) == SC_CLOSE as WPARAM))
        {
            unsafe {
                DestroyWindow(window);
            }
            break;
        }

        if message.message == WM_COMMAND {
            let control_id = loword(message.wParam);
            let notify_code = hiword(message.wParam);
            match control_id {
                ID_ENCODING_OK => {
                    if let Some(encoding) =
                        confirm_encoding_dialog_input(window, combo, status_label)?
                    {
                        selected = Some(encoding);
                        unsafe {
                            DestroyWindow(window);
                        }
                        break;
                    }
                    continue;
                }
                ID_ENCODING_CANCEL => {
                    unsafe {
                        DestroyWindow(window);
                    }
                    break;
                }
                ID_ENCODING_COMBO if matches!(notify_code, CBN_EDITCHANGE | CBN_SELCHANGE) => {
                    update_encoding_validation_status(combo, status_label)?;
                }
                _ => {}
            }
        }

        if matches!(message.message, WM_KEYDOWN | WM_SYSKEYDOWN) {
            match message.wParam as u32 {
                VK_RETURN_CODE => {
                    if let Some(encoding) =
                        confirm_encoding_dialog_input(window, combo, status_label)?
                    {
                        selected = Some(encoding);
                        unsafe {
                            DestroyWindow(window);
                        }
                        break;
                    }
                    continue;
                }
                VK_ESCAPE_CODE => {
                    unsafe {
                        DestroyWindow(window);
                    }
                    break;
                }
                _ => {}
            }
        }

        let ok_clicked = button_activated_by_message(ok_button, &message);
        let cancel_clicked = button_activated_by_message(cancel_button, &message);
        let handled = unsafe { IsDialogMessageW(window, &message) };
        if handled == 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }

        if unsafe { IsWindow(window) == 0 } {
            break;
        }
        if ok_clicked {
            if let Some(encoding) = confirm_encoding_dialog_input(window, combo, status_label)? {
                selected = Some(encoding);
                unsafe {
                    DestroyWindow(window);
                }
                break;
            }
            continue;
        }
        if cancel_clicked {
            unsafe {
                DestroyWindow(window);
            }
            break;
        }
        update_encoding_validation_status(combo, status_label)?;
    }

    Ok(selected)
}

fn create_encoding_dialog_window(owner: HWND, title: &str) -> Result<HWND, AppError> {
    let instance = unsafe { GetModuleHandleW(null()) };
    if instance.is_null() {
        return Err(last_win32_error("get module handle"));
    }

    let class = wide_null(CLASS_NAME);
    let title = wide_null(title);
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            520,
            250,
            owner,
            null_mut(),
            instance,
            null_mut(),
        )
    };
    if window.is_null() {
        return Err(last_win32_error("create encoding dialog window"));
    }
    Ok(window)
}

struct DialogControlSpec<'a> {
    class_name: &'a str,
    text: &'a str,
    bounds: (i32, i32, i32, i32),
    style: u32,
    ex_style: u32,
    id: u16,
}

fn create_dialog_control(parent: HWND, spec: DialogControlSpec<'_>) -> Result<HWND, AppError> {
    let class_name = wide_null(spec.class_name);
    let text = wide_null(spec.text);
    let (x, y, width, height) = spec.bounds;
    let control = unsafe {
        CreateWindowExW(
            spec.ex_style as WINDOW_EX_STYLE,
            class_name.as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | spec.style,
            x,
            y,
            width,
            height,
            parent,
            control_id(spec.id),
            null_mut(),
            null_mut(),
        )
    };
    if control.is_null() {
        return Err(last_win32_error("create dialog control"));
    }
    Ok(control)
}

fn fill_encoding_combo(combo: HWND, current: TextEncoding) -> Result<(), AppError> {
    unsafe {
        SendMessageW(combo, CB_LIMITTEXT, 64, 0);
    }
    for encoding in TextEncoding::ALL {
        combo_add_string(combo, encoding.display_name())?;
    }

    let current_index = TextEncoding::ALL
        .iter()
        .position(|encoding| *encoding == current)
        .unwrap_or(0);
    let result = unsafe { SendMessageW(combo, CB_SETCURSEL, current_index, 0) };
    if result == CB_ERR {
        return Err(last_win32_error("select encoding combo item"));
    }
    Ok(())
}

fn combo_add_string(combo: HWND, text: &str) -> Result<(), AppError> {
    let text = wide_null(text);
    let result = unsafe { SendMessageW(combo, CB_ADDSTRING, 0, text.as_ptr() as LPARAM) };
    if combo_add_string_failed(result) {
        return Err(last_win32_error("add encoding combo item"));
    }
    Ok(())
}

fn combo_add_string_failed(result: LRESULT) -> bool {
    result == CB_ERR || result == CB_ERRSPACE
}

fn button_activated_by_message(button: HWND, message: &MSG) -> bool {
    if message.hwnd != button {
        return false;
    }
    message.message == WM_LBUTTONUP
        || (message.message == WM_KEYDOWN && message.wParam as u32 == VK_SPACE_CODE)
}

fn message_targets_window_or_child(window: HWND, message: &MSG) -> bool {
    if window.is_null() || message.hwnd.is_null() {
        return false;
    }
    message.hwnd == window || unsafe { IsChild(window, message.hwnd) != 0 }
}

fn selected_encoding_from_dialog_input(
    combo: HWND,
    status_label: HWND,
) -> Result<Option<TextEncoding>, AppError> {
    update_encoding_validation_status(combo, status_label)
}

fn confirm_encoding_dialog_input(
    owner: HWND,
    combo: HWND,
    status_label: HWND,
) -> Result<Option<TextEncoding>, AppError> {
    let encoding = selected_encoding_from_dialog_input(combo, status_label)?;
    if encoding.is_none() {
        show_invalid_encoding_message(owner, combo)?;
    }
    Ok(encoding)
}

fn update_encoding_validation_status(
    combo: HWND,
    status_label: HWND,
) -> Result<Option<TextEncoding>, AppError> {
    let input = get_window_text(combo)?;
    let trimmed = input.trim();
    let encoding = TextEncoding::from_user_input(trimmed);
    let message = match encoding {
        Some(encoding) => format!("OK: {}", encoding.display_name()),
        None if trimmed.is_empty() => "Pick an encoding.".to_string(),
        None => "This encoding is not supported.".to_string(),
    };
    set_window_text(status_label, &message)?;
    Ok(encoding)
}

fn show_invalid_encoding_message(owner: HWND, combo: HWND) -> Result<(), AppError> {
    let input = get_window_text(combo)?;
    let trimmed = input.trim();
    let message = if trimmed.is_empty() {
        "Pick an encoding.".to_string()
    } else {
        format!("\"{trimmed}\" is not supported.\n\nTry UTF-8, CP949, or Windows-1252.")
    };
    message_box(owner, &message, "Encoding", MB_OK | MB_ICONWARNING);
    unsafe {
        SetFocus(combo);
    }
    Ok(())
}

struct WindowDestroyGuard {
    hwnd: HWND,
}

impl WindowDestroyGuard {
    fn new(hwnd: HWND) -> Self {
        Self { hwnd }
    }
}

impl Drop for WindowDestroyGuard {
    fn drop(&mut self) {
        if !self.hwnd.is_null() && unsafe { IsWindow(self.hwnd) != 0 } {
            unsafe {
                DestroyWindow(self.hwnd);
            }
        }
    }
}

struct CustomModalDialogGuard {
    owner: HWND,
    _active_modal: ActiveModalDialogGuard,
}

impl CustomModalDialogGuard {
    fn install(owner: HWND) -> Self {
        let active_modal = ActiveModalDialogGuard::enter();
        if !owner.is_null() {
            unsafe {
                EnableWindow(owner, 0);
            }
        }
        Self {
            owner,
            _active_modal: active_modal,
        }
    }
}

impl Drop for CustomModalDialogGuard {
    fn drop(&mut self) {
        if !self.owner.is_null() {
            unsafe {
                EnableWindow(self.owner, 1);
                SetFocus(self.owner);
            }
        }
    }
}

fn choose_font_dialog(
    owner: HWND,
    settings: &EditorSettings,
) -> Result<Option<EditorSettings>, AppError> {
    let mut updated = settings.clone().sanitized();
    let mut logfont = LOGFONTW {
        lfHeight: points_to_logical_height(updated.font_size_pt, dpi_y_for_window(owner)),
        lfWeight: 400,
        lfCharSet: 1,
        ..LOGFONTW::default()
    };
    set_logfont_face_name(&mut logfont, &updated.font_name);

    let mut choose_font = CHOOSEFONTW {
        lStructSize: size_of::<CHOOSEFONTW>() as u32,
        hwndOwner: owner,
        lpLogFont: &mut logfont,
        iPointSize: (updated.font_size_pt.saturating_mul(10)) as i32,
        Flags: CF_SCREENFONTS
            | CF_INITTOLOGFONTSTRUCT
            | CF_FORCEFONTEXIST
            | CF_LIMITSIZE
            | CF_NOSTYLESEL
            | CF_NOSCRIPTSEL
            | CF_NOVERTFONTS,
        nSizeMin: MIN_FONT_SIZE_PT as i32,
        nSizeMax: MAX_FONT_SIZE_PT as i32,
        ..CHOOSEFONTW::default()
    };

    let ok = with_centered_dialog(owner, || unsafe { ChooseFontW(&mut choose_font) });
    if ok == 0 {
        let code = unsafe { CommDlgExtendedError() };
        if code == 0 {
            return Ok(None);
        }
        return Err(AppError::dialog(code, "font dialog"));
    }

    if let Some(font_name) = logfont_face_name(&logfont) {
        updated.font_name = font_name;
    }
    updated.font_size_pt = selected_point_size(owner, choose_font.iPointSize, logfont.lfHeight);
    Ok(Some(updated.sanitized()))
}

fn capture_shortcut_dialog(
    owner: HWND,
    command_title: &str,
) -> Result<Option<KeyboardShortcut>, AppError> {
    let class = wide_null("STATIC");
    let title = wide_null("Set Shortcut");
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            420,
            160,
            owner,
            null_mut(),
            null_mut(),
            null_mut(),
        )
    };
    if window.is_null() {
        return Err(last_win32_error("create shortcut capture window"));
    }
    let _window_guard = WindowDestroyGuard::new(window);

    let prompt = wide_null(&format!(
        "Press keys for {command_title}.\r\nUse Ctrl, Alt, or an F-key. Esc cancels."
    ));
    let label_class = wide_null("STATIC");
    let label = unsafe {
        CreateWindowExW(
            0,
            label_class.as_ptr(),
            prompt.as_ptr(),
            WS_CHILD | WS_VISIBLE,
            16,
            16,
            372,
            48,
            window,
            null_mut(),
            null_mut(),
            null_mut(),
        )
    };
    if label.is_null() {
        unsafe {
            DestroyWindow(window);
        }
        return Err(last_win32_error("create shortcut capture label"));
    }

    let button_class = wide_null("BUTTON");
    let cancel_text = wide_null("Cancel");
    let cancel = unsafe {
        CreateWindowExW(
            0,
            button_class.as_ptr(),
            cancel_text.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
            300,
            82,
            88,
            28,
            window,
            control_id(ID_SHORTCUT_CAPTURE_CANCEL),
            null_mut(),
            null_mut(),
        )
    };
    if cancel.is_null() {
        unsafe {
            DestroyWindow(window);
        }
        return Err(last_win32_error("create shortcut capture cancel button"));
    }

    center_window_over_owner(window, owner);
    let _modal_guard = CustomModalDialogGuard::install(owner);
    unsafe {
        ShowWindow(window, SW_SHOW);
        SetFocus(cancel);
    }

    let mut captured = None;
    let mut message = unsafe { MaybeUninit::<MSG>::zeroed().assume_init() };
    while unsafe { IsWindow(window) != 0 } {
        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if result == -1 {
            unsafe {
                DestroyWindow(window);
            }
            return Err(last_win32_error("get shortcut capture message"));
        }
        if result == 0 {
            unsafe {
                PostQuitMessage(0);
            }
            break;
        }

        if message_targets_window_or_child(owner, &message) {
            continue;
        }

        if matches!(message.message, WM_KEYDOWN | WM_SYSKEYDOWN)
            && !key_message_is_repeat(message.lParam)
        {
            let virtual_key = message.wParam as u32;
            if virtual_key == VK_ESCAPE_CODE {
                unsafe {
                    DestroyWindow(window);
                }
                break;
            }
            if let Some(shortcut) = shortcut_from_virtual_key(virtual_key) {
                captured = Some(shortcut);
                unsafe {
                    DestroyWindow(window);
                }
                break;
            }
            if shortcut_key_from_virtual_key(virtual_key).is_some() {
                message_box(
                    window,
                    "Use Ctrl, Alt, or an F-key.",
                    "Shortcut",
                    MB_OK | MB_ICONWARNING,
                );
                continue;
            }
        }

        if message.message == WM_COMMAND && loword(message.wParam) == ID_SHORTCUT_CAPTURE_CANCEL {
            unsafe {
                DestroyWindow(window);
            }
            break;
        }

        let handled = unsafe { IsDialogMessageW(window, &message) };
        if handled == 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    Ok(captured)
}

fn dropped_paths(drop: HDROP) -> Vec<PathBuf> {
    let count = unsafe { DragQueryFileW(drop, u32::MAX, null_mut(), 0) };
    let mut paths = Vec::new();
    for index in 0..count {
        let len = unsafe { DragQueryFileW(drop, index, null_mut(), 0) };
        if len == 0 {
            continue;
        }
        let mut buffer = vec![0u16; len as usize + 1];
        let copied = unsafe { DragQueryFileW(drop, index, buffer.as_mut_ptr(), len + 1) };
        if copied == 0 {
            continue;
        }
        let os = OsString::from_wide(&buffer[..copied as usize]);
        paths.push(PathBuf::from(os));
    }
    paths
}

struct DropFilesGuard {
    handle: HDROP,
}

impl DropFilesGuard {
    fn new(handle: HDROP) -> Self {
        Self { handle }
    }

    fn handle(&self) -> HDROP {
        self.handle
    }

    fn finish(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                DragFinish(self.handle);
            }
            self.handle = null_mut();
        }
    }
}

impl Drop for DropFilesGuard {
    fn drop(&mut self) {
        self.finish();
    }
}

struct MenuGuard {
    handle: HMENU,
}

impl MenuGuard {
    fn create_menu(context: &'static str) -> Result<Self, AppError> {
        let handle = unsafe { CreateMenu() };
        if handle.is_null() {
            return Err(last_win32_error(context));
        }
        Ok(Self { handle })
    }

    fn create_popup(context: &'static str) -> Result<Self, AppError> {
        let handle = unsafe { CreatePopupMenu() };
        if handle.is_null() {
            return Err(last_win32_error(context));
        }
        Ok(Self { handle })
    }

    fn handle(&self) -> HMENU {
        self.handle
    }

    fn release(&mut self) {
        self.handle = null_mut();
    }
}

impl Drop for MenuGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                DestroyMenu(self.handle);
            }
        }
    }
}

fn append_item(menu: HMENU, id: u16, text: &str) -> Result<(), AppError> {
    let text = wide_null(&menu_text(text));
    let ok = unsafe { AppendMenuW(menu, MF_STRING, id as usize, text.as_ptr()) };
    if ok == 0 {
        return Err(last_win32_error("append menu item"));
    }
    Ok(())
}

fn append_disabled_item(menu: HMENU, text: &str) -> Result<(), AppError> {
    let text = wide_null(&menu_text(text));
    let ok = unsafe { AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, text.as_ptr()) };
    if ok == 0 {
        return Err(last_win32_error("append disabled menu item"));
    }
    Ok(())
}

fn append_popup(menu: HMENU, popup: HMENU, text: &str) -> Result<(), AppError> {
    let text = wide_null(&menu_text(text));
    let ok = unsafe { AppendMenuW(menu, MF_POPUP, popup as usize, text.as_ptr()) };
    if ok == 0 {
        return Err(last_win32_error("append menu popup"));
    }
    Ok(())
}

fn append_separator(menu: HMENU) -> Result<(), AppError> {
    let ok = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, null()) };
    if ok == 0 {
        return Err(last_win32_error("append menu separator"));
    }
    Ok(())
}

fn check_menu(menu: HMENU, id: u16, checked: bool) {
    let state = if checked { MF_CHECKED } else { MF_UNCHECKED };
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::CheckMenuItem(
            menu,
            id as u32,
            MF_BYCOMMAND | state,
        );
    }
}

fn enable_menu_item(menu: HMENU, id: u16, enabled: bool) {
    let state = if enabled { MF_ENABLED } else { MF_GRAYED };
    unsafe {
        EnableMenuItem(menu, id as u32, MF_BYCOMMAND | state);
    }
}

fn listbox_add_string(hwnd: HWND, text: &str) -> Result<(), AppError> {
    let text = wide_null(text);
    let result = unsafe { SendMessageW(hwnd, LB_ADDSTRING, 0, text.as_ptr() as LPARAM) };
    if listbox_add_string_failed(result) {
        return Err(last_win32_error("add listbox item"));
    }
    Ok(())
}

fn listbox_add_string_failed(result: LRESULT) -> bool {
    result == LB_ERR || result == LB_ERRSPACE
}

fn set_rich_edit_plain_text_mode(hwnd: HWND) -> Result<(), AppError> {
    if rich_edit_plain_text_length(hwnd)? != 0 {
        return Err(AppError::platform(
            PlatformErrorKind::RichEditPlainTextModeFailed,
            "set Rich Edit plain text mode",
            "Rich Edit plain text mode must be set before loading document text",
        ));
    }

    let result = unsafe { SendMessageW(hwnd, EM_SETTEXTMODE_LOCAL, RICH_EDIT_TEXT_MODE, 0) };
    if result != 0 {
        return Err(AppError::platform(
            PlatformErrorKind::RichEditPlainTextModeFailed,
            "set Rich Edit plain text mode",
            "Failed to set Rich Edit plain text mode before loading document text",
        ));
    }
    Ok(())
}

fn set_rich_edit_event_mask(hwnd: HWND) {
    let event_mask = ENM_CHANGE_LOCAL | ENM_SCROLL_LOCAL | ENM_SELCHANGE_LOCAL;
    unsafe {
        SendMessageW(hwnd, EM_SETEVENTMASK_LOCAL, 0, event_mask);
    }
}

fn set_rich_edit_text_limit(hwnd: HWND, limit: LPARAM) {
    unsafe {
        SendMessageW(hwnd, EM_EXLIMITTEXT_LOCAL, 0, limit);
    }
}

fn apply_rich_edit_presentation_text_color(hwnd: HWND, color: COLORREF) {
    let format = RichEditCharFormatW {
        cb_size: size_of::<RichEditCharFormatW>() as u32,
        mask: CFM_COLOR_LOCAL,
        effects: 0,
        height: 0,
        offset: 0,
        text_color: color,
        charset: 0,
        pitch_and_family: 0,
        face_name: [0; LF_FACESIZE_LOCAL],
    };
    unsafe {
        SendMessageW(
            hwnd,
            EM_SETCHARFORMAT_LOCAL,
            SCF_DEFAULT_LOCAL,
            (&format as *const RichEditCharFormatW) as LPARAM,
        );
        SendMessageW(
            hwnd,
            EM_SETCHARFORMAT_LOCAL,
            SCF_ALL_LOCAL,
            (&format as *const RichEditCharFormatW) as LPARAM,
        );
    }
}

fn command_label(command: EditorCommand) -> String {
    format!("{}: {}", command.group.display_name(), command.title)
}

fn record_command_palette_id_after_listbox_add(
    filtered_command_ids: &mut Vec<EditorCommandId>,
    command_id: EditorCommandId,
    add_result: Result<(), AppError>,
) -> Result<(), AppError> {
    add_result?;
    filtered_command_ids.push(command_id);
    Ok(())
}

fn menu_command_id_for_editor_command(command: EditorCommandId) -> Option<u16> {
    match command {
        EditorCommandId::NewFile => Some(ID_FILE_NEW),
        EditorCommandId::OpenFile => Some(ID_FILE_OPEN),
        EditorCommandId::Save => Some(ID_FILE_SAVE),
        EditorCommandId::SaveAs => Some(ID_FILE_SAVE_AS),
        EditorCommandId::CloseTab => Some(ID_FILE_CLOSE_TAB),
        EditorCommandId::CloseOtherTabs => Some(ID_FILE_CLOSE_OTHER_TABS),
        EditorCommandId::Undo => Some(ID_EDIT_UNDO),
        EditorCommandId::Redo => Some(ID_EDIT_REDO),
        EditorCommandId::Cut => Some(ID_EDIT_CUT),
        EditorCommandId::Copy => Some(ID_EDIT_COPY),
        EditorCommandId::Paste => Some(ID_EDIT_PASTE),
        EditorCommandId::SelectAll => Some(ID_EDIT_SELECT_ALL),
        EditorCommandId::Find => Some(ID_EDIT_FIND),
        EditorCommandId::Replace => Some(ID_EDIT_REPLACE),
        EditorCommandId::FindAll => Some(ID_EDIT_FIND_ALL),
        EditorCommandId::FindNext => Some(ID_EDIT_FIND_NEXT),
        EditorCommandId::FindPrevious => Some(ID_EDIT_FIND_PREVIOUS),
        EditorCommandId::CommandPalette => Some(ID_VIEW_COMMAND_PALETTE),
        EditorCommandId::ToggleLineNumbers => Some(ID_VIEW_LINE_NUMBERS),
        EditorCommandId::ToggleVisibleWhitespace => Some(ID_VIEW_WHITESPACE),
        EditorCommandId::ToggleWordWrap => Some(ID_SETTINGS_WORD_WRAP),
        EditorCommandId::ReopenWithEncoding => Some(ID_ENCODING_REOPEN),
        EditorCommandId::ConvertEncoding => Some(ID_ENCODING_CONVERT),
        EditorCommandId::SetLineEnding(LineEnding::Crlf) => Some(ID_LINE_ENDING_CRLF),
        EditorCommandId::SetLineEnding(LineEnding::Lf) => Some(ID_LINE_ENDING_LF),
        EditorCommandId::SetLineEnding(LineEnding::Cr) => Some(ID_LINE_ENDING_CR),
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

fn editor_save_state_label(status: &crate::app::CurrentEditorStatus) -> &'static str {
    if status.can_save {
        "Can Save"
    } else if status.can_save_as {
        "Save As"
    } else {
        "No Save"
    }
}

fn editor_status_state_text(status: &crate::app::CurrentEditorStatus) -> String {
    let label = status.status_kind.label();
    let mut state =
        String::with_capacity(label.len() + 3 + status.title.len().max(APP_TITLE.len()));
    state.push_str(label);
    state.push_str(" | ");
    if let Some(path) = status.path.as_ref() {
        let _ = write!(&mut state, "{}", path.display());
    } else if status.title.is_empty() {
        state.push_str(APP_TITLE);
    } else {
        state.push_str(&status.title);
    }
    state
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

fn append_shortcut_settings_items(
    menu: HMENU,
    shortcuts: &crate::domain::EditorShortcuts,
) -> Result<(), AppError> {
    for (index, command) in EditorCommandId::SHORTCUT_COMMANDS
        .iter()
        .copied()
        .enumerate()
    {
        let mut command_menu_guard = MenuGuard::create_popup("create shortcut submenu")?;
        let command_menu = command_menu_guard.handle();
        let current = shortcuts.shortcut_for(command);
        let default = command.default_shortcut();
        append_disabled_item(
            command_menu,
            &format!("Now: {}", optional_shortcut_display_name(current)),
        )?;
        append_separator(command_menu)?;
        append_item(
            command_menu,
            shortcut_settings_command_id(index, ShortcutMenuAction::Capture),
            "Set...",
        )?;
        append_item(
            command_menu,
            shortcut_settings_command_id(index, ShortcutMenuAction::UseDefault),
            &format!("Default: {}", optional_shortcut_display_name(default)),
        )?;
        append_item(
            command_menu,
            shortcut_settings_command_id(index, ShortcutMenuAction::Disable),
            "Off",
        )?;
        append_popup(
            menu,
            command_menu,
            &format!(
                "{} ({})",
                command.shortcut_title().unwrap_or("Command"),
                optional_shortcut_display_name(current)
            ),
        )?;
        command_menu_guard.release();
    }
    Ok(())
}

fn recent_index_from_command(command_id: u16) -> Option<usize> {
    if (ID_FILE_RECENT_BASE..ID_FILE_RECENT_BASE + 10).contains(&command_id) {
        Some((command_id - ID_FILE_RECENT_BASE) as usize)
    } else {
        None
    }
}

fn shortcut_settings_command_id(index: usize, action: ShortcutMenuAction) -> u16 {
    let action_index = match action {
        ShortcutMenuAction::Capture => 0,
        ShortcutMenuAction::UseDefault => 1,
        ShortcutMenuAction::Disable => 2,
    };
    ID_SETTINGS_SHORTCUT_BASE + (index as u16 * SHORTCUT_MENU_ACTION_COUNT) + action_index
}

fn shortcut_settings_from_command(
    command_id: u16,
) -> Option<(EditorCommandId, ShortcutMenuAction)> {
    let offset = command_id.checked_sub(ID_SETTINGS_SHORTCUT_BASE)?;
    let index = (offset / SHORTCUT_MENU_ACTION_COUNT) as usize;
    let action = match offset % SHORTCUT_MENU_ACTION_COUNT {
        0 => ShortcutMenuAction::Capture,
        1 => ShortcutMenuAction::UseDefault,
        2 => ShortcutMenuAction::Disable,
        _ => return None,
    };
    let command = EditorCommandId::SHORTCUT_COMMANDS.get(index).copied()?;
    Some((command, action))
}

struct RichEditPlainText {
    text: String,
    metrics: DocumentMetrics,
    char_count: usize,
}

#[cfg(test)]
fn get_rich_edit_plain_text(hwnd: HWND) -> Result<RichEditPlainText, AppError> {
    get_rich_edit_plain_text_with_limit(hwnd, None)
}

fn get_rich_edit_plain_text_for_document_sync(hwnd: HWND) -> Result<RichEditPlainText, AppError> {
    get_rich_edit_plain_text_with_limit(hwnd, Some(MAX_DOCUMENT_LOAD_BYTES))
}

fn get_rich_edit_plain_text_with_limit(
    hwnd: HWND,
    max_utf8_bytes: Option<u64>,
) -> Result<RichEditPlainText, AppError> {
    if let Some(text) = try_get_rich_edit_plain_text_utf8_with_limit(hwnd, max_utf8_bytes)? {
        return Ok(text);
    }

    get_rich_edit_plain_text_utf16_with_limit(hwnd, max_utf8_bytes)
}

fn try_get_rich_edit_plain_text_utf8_with_limit(
    hwnd: HWND,
    max_utf8_bytes: Option<u64>,
) -> Result<Option<RichEditPlainText>, AppError> {
    const MAX_ATTEMPTS: usize = 3;

    for _ in 0..MAX_ATTEMPTS {
        let length = rich_edit_plain_text_length(hwnd)?;
        validate_rich_edit_text_units(length)?;
        let Some(byte_len) = try_rich_edit_plain_text_utf8_byte_len(hwnd)? else {
            return Ok(None);
        };
        validate_rich_edit_utf8_byte_len(byte_len, max_utf8_bytes)?;

        let Some(buffer_len) = byte_len.checked_add(1) else {
            return Ok(None);
        };
        let Ok(buffer_len_usize) = usize::try_from(buffer_len) else {
            return Ok(None);
        };
        let Ok(buffer_len_u32) = u32::try_from(buffer_len) else {
            return Ok(None);
        };
        let mut buffer = vec![0u8; buffer_len_usize];
        let text_options = RichEditGetTextEx {
            cb: buffer_len_u32,
            flags: GT_USECRLF_LOCAL,
            codepage: CP_UTF8_LOCAL,
            default_char: null(),
            used_default_char: null_mut(),
        };
        // The buffer is allocated for GETTEXTEX.cb bytes plus the terminating NUL.
        let copied = unsafe {
            SendMessageW(
                hwnd,
                EM_GETTEXTEX_LOCAL,
                (&text_options as *const RichEditGetTextEx) as WPARAM,
                buffer.as_mut_ptr() as LPARAM,
            )
        };
        if copied < 0 {
            return Ok(None);
        }

        let copied = copied as usize;
        let Ok(byte_len_usize) = usize::try_from(byte_len) else {
            return Ok(None);
        };
        if copied > byte_len_usize {
            return Ok(None);
        }
        let Some(current_byte_len) = try_rich_edit_plain_text_utf8_byte_len(hwnd)? else {
            return Ok(None);
        };
        let copied_u64 = u64::try_from(copied).map_err(|_| {
            AppError::InvalidState("Rich Edit copied text length does not fit in u64")
        })?;
        if current_byte_len > copied_u64 {
            continue;
        }

        buffer.truncate(copied);
        let Ok(text) = String::from_utf8(buffer) else {
            return Ok(None);
        };
        let char_count = validate_plain_text_control_text_and_count_chars(&text)?;
        return Ok(Some(RichEditPlainText {
            text,
            metrics: DocumentMetrics::from_char_count(char_count),
            char_count,
        }));
    }

    Ok(None)
}

fn get_rich_edit_plain_text_utf16_with_limit(
    hwnd: HWND,
    max_utf8_bytes: Option<u64>,
) -> Result<RichEditPlainText, AppError> {
    const MAX_ATTEMPTS: usize = 3;

    for _ in 0..MAX_ATTEMPTS {
        let length = rich_edit_plain_text_length(hwnd)?;
        validate_rich_edit_text_units(length)?;
        let buffer_units = length.checked_add(1).ok_or(AppError::InvalidState(
            "Rich Edit text is too large to read",
        ))?;
        let byte_len = rich_edit_buffer_byte_len(buffer_units)?;
        let mut buffer = vec![0u16; buffer_units];
        let text_options = RichEditGetTextEx {
            cb: byte_len,
            flags: GT_USECRLF_LOCAL,
            codepage: CP_UNICODE_LOCAL,
            default_char: null(),
            used_default_char: null_mut(),
        };
        let copied = unsafe {
            SendMessageW(
                hwnd,
                EM_GETTEXTEX_LOCAL,
                (&text_options as *const RichEditGetTextEx) as WPARAM,
                buffer.as_mut_ptr() as LPARAM,
            )
        };
        if copied < 0 {
            return Err(AppError::InvalidState("Could not read editor text."));
        }

        let copied = copied as usize;
        if copied > length {
            return Err(AppError::InvalidState(
                "Rich Edit text copy exceeded the allocated buffer",
            ));
        }
        if rich_edit_plain_text_length(hwnd)? > copied {
            continue;
        }

        buffer.truncate(copied);
        return decode_rich_edit_plain_text(&buffer, max_utf8_bytes);
    }

    Err(AppError::InvalidState("Text changed while reading."))
}

fn decode_rich_edit_plain_text(
    units: &[u16],
    max_utf8_bytes: Option<u64>,
) -> Result<RichEditPlainText, AppError> {
    let mut text = String::with_capacity(units.len());
    let mut char_count = 0;
    let mut byte_len = 0u64;
    let mut consumed_units = 0usize;
    let mut utf8_capacity_reserved = false;

    for decoded in std::char::decode_utf16(units.iter().copied()) {
        let ch = decoded
            .map_err(|_| AppError::encoding_decode("Rich Edit text contains invalid UTF-16"))?;
        if ch == '\0' {
            return Err(plain_text_control_nul_error());
        }
        let ch_utf8_len = ch.len_utf8();
        byte_len = byte_len
            .checked_add(ch_utf8_len as u64)
            .ok_or(AppError::InvalidState("Text is too large."))?;
        if let Some(max_utf8_bytes) = max_utf8_bytes
            && byte_len > max_utf8_bytes
        {
            return Err(AppError::InvalidState(
                EDITOR_DOCUMENT_TEXT_TOO_LARGE_MESSAGE,
            ));
        }
        if !utf8_capacity_reserved && ch_utf8_len > ch.len_utf16() {
            reserve_rich_edit_utf8_capacity(&mut text, &units[consumed_units..], max_utf8_bytes);
            utf8_capacity_reserved = true;
        }
        text.push(ch);
        char_count += 1;
        consumed_units += ch.len_utf16();
    }

    Ok(RichEditPlainText {
        text,
        metrics: DocumentMetrics::from_char_count(char_count),
        char_count,
    })
}

fn reserve_rich_edit_utf8_capacity(
    text: &mut String,
    remaining_units: &[u16],
    max_utf8_bytes: Option<u64>,
) {
    let Some(remaining_utf8_bytes) = rich_edit_utf8_capacity_from_utf16_units(remaining_units)
    else {
        return;
    };
    let Some(required_capacity) = text.len().checked_add(remaining_utf8_bytes) else {
        return;
    };
    // Keep UTF-16 validation and max-size error precedence in the decode loop.
    let capped_capacity = match max_utf8_bytes.and_then(|max| usize::try_from(max).ok()) {
        Some(max_utf8_bytes) => required_capacity.min(max_utf8_bytes),
        None => required_capacity,
    };

    if capped_capacity > text.capacity() {
        text.reserve_exact(capped_capacity - text.capacity());
    }
}

fn rich_edit_utf8_capacity_from_utf16_units(units: &[u16]) -> Option<usize> {
    let mut byte_len = 0usize;
    let mut index = 0usize;

    while index < units.len() {
        let unit = units[index];
        let utf8_width = if unit <= 0x007F {
            1
        } else if unit <= 0x07FF {
            2
        } else if (0xD800..=0xDBFF).contains(&unit)
            && units
                .get(index + 1)
                .is_some_and(|next| (0xDC00..=0xDFFF).contains(next))
        {
            index += 1;
            4
        } else {
            3
        };
        byte_len = byte_len.checked_add(utf8_width)?;
        index += 1;
    }

    Some(byte_len)
}

fn rich_edit_plain_text_length(hwnd: HWND) -> Result<usize, AppError> {
    let length_options = RichEditGetTextLengthEx {
        flags: GTL_USECRLF_LOCAL | GTL_PRECISE_LOCAL | GTL_NUMCHARS_LOCAL,
        codepage: CP_UNICODE_LOCAL,
    };
    let length = unsafe {
        SendMessageW(
            hwnd,
            EM_GETTEXTLENGTHEX_LOCAL,
            (&length_options as *const RichEditGetTextLengthEx) as WPARAM,
            0,
        )
    };
    if length < 0 {
        return Err(AppError::InvalidState(
            "Failed to get Rich Edit plain text length",
        ));
    }
    Ok(length as usize)
}

#[cfg(test)]
fn rich_edit_plain_text_utf8_byte_len(hwnd: HWND) -> Result<u64, AppError> {
    try_rich_edit_plain_text_utf8_byte_len(hwnd)?.ok_or(AppError::InvalidState(
        "Failed to get Rich Edit plain text byte length",
    ))
}

fn try_rich_edit_plain_text_utf8_byte_len(hwnd: HWND) -> Result<Option<u64>, AppError> {
    let length_options = RichEditGetTextLengthEx {
        flags: GTL_USECRLF_LOCAL | GTL_PRECISE_LOCAL | GTL_NUMBYTES_LOCAL,
        codepage: CP_UTF8_LOCAL,
    };
    let byte_len = unsafe {
        SendMessageW(
            hwnd,
            EM_GETTEXTLENGTHEX_LOCAL,
            (&length_options as *const RichEditGetTextLengthEx) as WPARAM,
            0,
        )
    };
    if byte_len < 0 {
        return Ok(None);
    }
    u64::try_from(byte_len)
        .map(Some)
        .map_err(|_| AppError::InvalidState("Rich Edit plain text byte length does not fit in u64"))
}

fn set_rich_edit_plain_text(hwnd: HWND, text: &str) -> Result<(), AppError> {
    validate_plain_text_control_text(text)?;
    let text_options = RichEditSetTextEx {
        flags: ST_UNICODE_LOCAL | ST_PLAINTEXTONLY_LOCAL,
        codepage: CP_UNICODE_LOCAL,
    };
    let text = wide_null(text);
    let result = unsafe {
        SendMessageW(
            hwnd,
            EM_SETTEXTEX_LOCAL,
            (&text_options as *const RichEditSetTextEx) as WPARAM,
            text.as_ptr() as LPARAM,
        )
    };
    if result == 0 {
        return Err(AppError::InvalidState("Could not set editor text."));
    }
    Ok(())
}

fn replace_rich_edit_selection_plain_text(hwnd: HWND, text: &str) -> Result<(), AppError> {
    validate_plain_text_control_text(text)?;
    let is_empty = text.is_empty();
    let text_options = RichEditSetTextEx {
        flags: ST_SELECTION_LOCAL | ST_KEEPUNDO_LOCAL | ST_UNICODE_LOCAL | ST_PLAINTEXTONLY_LOCAL,
        codepage: CP_UNICODE_LOCAL,
    };
    let text = wide_null(text);
    let result = unsafe {
        SendMessageW(
            hwnd,
            EM_SETTEXTEX_LOCAL,
            (&text_options as *const RichEditSetTextEx) as WPARAM,
            text.as_ptr() as LPARAM,
        )
    };
    if !is_empty && result == 0 {
        return Err(AppError::InvalidState("Could not replace text."));
    }
    Ok(())
}

fn rich_edit_buffer_byte_len(buffer_units: usize) -> Result<u32, AppError> {
    buffer_units
        .checked_mul(size_of::<u16>())
        .and_then(|bytes| u32::try_from(bytes).ok())
        .ok_or(AppError::InvalidState(
            "Rich Edit text is too large to fit in a Win32 buffer",
        ))
}

fn validate_rich_edit_text_units(length: usize) -> Result<(), AppError> {
    if length > RICH_EDIT_SURFACE_TEXT_LIMIT_UNITS {
        return Err(AppError::InvalidState(
            RICH_EDIT_SURFACE_TEXT_TOO_LARGE_MESSAGE,
        ));
    }
    Ok(())
}

fn validate_rich_edit_utf8_byte_len(
    byte_len: u64,
    max_utf8_bytes: Option<u64>,
) -> Result<(), AppError> {
    if max_utf8_bytes.is_some_and(|max_utf8_bytes| byte_len > max_utf8_bytes) {
        return Err(AppError::InvalidState(
            EDITOR_DOCUMENT_TEXT_TOO_LARGE_MESSAGE,
        ));
    }
    Ok(())
}

fn validate_editor_document_text_size(text: &str) -> Result<(), AppError> {
    let byte_len = editor_document_byte_len_from_usize(text.len())?;
    validate_editor_document_byte_len(byte_len)
}

struct ReplaceAllText {
    text: String,
    byte_len: u64,
    metrics: DocumentMetrics,
}

fn replace_all_text_if_changed(
    text: &str,
    source_char_count: usize,
    query: &str,
    replacement: &str,
) -> Result<Option<ReplaceAllText>, AppError> {
    replace_all_text_if_changed_with_policy(
        text,
        source_char_count,
        query,
        replacement,
        can_load_document_bytes,
    )
}

fn replace_all_text_if_changed_with_policy(
    text: &str,
    source_char_count: usize,
    query: &str,
    replacement: &str,
    can_load_result_bytes: impl Fn(u64) -> bool,
) -> Result<Option<ReplaceAllText>, AppError> {
    if query.is_empty() || query == replacement {
        return Ok(None);
    }

    let mut matches = text.match_indices(query);
    let Some((first_match_start, first_match)) = matches.next() else {
        return Ok(None);
    };

    let mut replaced_text =
        String::with_capacity(replace_all_initial_capacity(text, query, replacement));
    let mut byte_len = 0;
    let mut cursor = 0;
    let mut match_count = 0usize;

    for (match_start, matched) in std::iter::once((first_match_start, first_match)).chain(matches) {
        append_replace_all_part(
            &mut replaced_text,
            &mut byte_len,
            &text[cursor..match_start],
            &can_load_result_bytes,
        )?;
        append_replace_all_part(
            &mut replaced_text,
            &mut byte_len,
            replacement,
            &can_load_result_bytes,
        )?;
        cursor = match_start + matched.len();
        match_count = checked_replace_all_match_count_add(match_count)?;
    }

    append_replace_all_part(
        &mut replaced_text,
        &mut byte_len,
        &text[cursor..],
        &can_load_result_bytes,
    )?;
    shrink_replace_all_text_if_significantly_smaller(&mut replaced_text);
    let metrics = replace_all_result_metrics(
        source_char_count,
        query.chars().count(),
        replacement.chars().count(),
        match_count,
    )?;
    debug_assert_eq!(
        byte_len,
        editor_document_byte_len_from_usize(replaced_text.len())?
    );

    Ok(Some(ReplaceAllText {
        text: replaced_text,
        byte_len,
        metrics,
    }))
}

fn replace_all_initial_capacity(text: &str, query: &str, replacement: &str) -> usize {
    text.len()
        .saturating_sub(query.len().saturating_sub(replacement.len()))
}

fn append_replace_all_part(
    replaced_text: &mut String,
    byte_len: &mut u64,
    part: &str,
    can_load_result_bytes: &impl Fn(u64) -> bool,
) -> Result<(), AppError> {
    *byte_len = checked_replace_all_result_byte_len_append(*byte_len, part, can_load_result_bytes)?;
    replaced_text.push_str(part);
    Ok(())
}

fn shrink_replace_all_text_if_significantly_smaller(text: &mut String) {
    if text.len() < text.capacity() / 2 {
        text.shrink_to_fit();
    }
}

fn checked_replace_all_match_count_add(match_count: usize) -> Result<usize, AppError> {
    match_count.checked_add(1).ok_or(AppError::InvalidState(
        REPLACE_ALL_RESULT_TEXT_LENGTH_OVERFLOW_MESSAGE,
    ))
}

fn replace_all_result_metrics(
    source_char_count: usize,
    query_char_count: usize,
    replacement_char_count: usize,
    match_count: usize,
) -> Result<DocumentMetrics, AppError> {
    let removed_char_count =
        checked_replace_all_result_char_count_mul(query_char_count, match_count)?;
    let inserted_char_count =
        checked_replace_all_result_char_count_mul(replacement_char_count, match_count)?;
    let kept_char_count =
        source_char_count
            .checked_sub(removed_char_count)
            .ok_or(AppError::InvalidState(
                "Replace All source character count is inconsistent",
            ))?;
    let char_count =
        checked_replace_all_result_char_count_add(kept_char_count, inserted_char_count)?;

    Ok(DocumentMetrics::from_char_count(char_count))
}

fn checked_replace_all_result_char_count_mul(
    char_count: usize,
    match_count: usize,
) -> Result<usize, AppError> {
    char_count
        .checked_mul(match_count)
        .ok_or(AppError::InvalidState(
            REPLACE_ALL_RESULT_TEXT_LENGTH_OVERFLOW_MESSAGE,
        ))
}

fn checked_replace_all_result_char_count_add(
    current_char_count: usize,
    added_char_count: usize,
) -> Result<usize, AppError> {
    current_char_count
        .checked_add(added_char_count)
        .ok_or(AppError::InvalidState(
            REPLACE_ALL_RESULT_TEXT_LENGTH_OVERFLOW_MESSAGE,
        ))
}

fn checked_replace_all_result_byte_len_append(
    current_byte_len: u64,
    part: &str,
    can_load_result_bytes: &impl Fn(u64) -> bool,
) -> Result<u64, AppError> {
    let part_byte_len = editor_document_byte_len_from_usize(part.len())?;
    let byte_len = checked_replace_all_result_byte_len_add(current_byte_len, part_byte_len)?;
    validate_replace_all_result_byte_len(byte_len, can_load_result_bytes)?;
    Ok(byte_len)
}

fn validate_replace_all_result_byte_len(
    byte_len: u64,
    can_load_result_bytes: &impl Fn(u64) -> bool,
) -> Result<(), AppError> {
    if !can_load_result_bytes(byte_len) {
        return Err(AppError::InvalidState(
            EDITOR_DOCUMENT_TEXT_TOO_LARGE_MESSAGE,
        ));
    }
    Ok(())
}

fn checked_replace_all_result_byte_len_add(
    current_byte_len: u64,
    added_byte_len: u64,
) -> Result<u64, AppError> {
    current_byte_len
        .checked_add(added_byte_len)
        .ok_or(AppError::InvalidState(
            REPLACE_ALL_RESULT_TEXT_LENGTH_OVERFLOW_MESSAGE,
        ))
}

fn rich_edit_selection_byte_range(
    text: &str,
    selection_start: usize,
    selection_end: usize,
) -> Result<(usize, usize), AppError> {
    let start = selection_start.min(selection_end);
    let end = selection_start.max(selection_end);
    let (start_byte, end_byte) = rich_edit_offset_range_to_byte_indices(text, start, end);
    if start_byte > end_byte {
        return Err(AppError::InvalidState("Selection is invalid."));
    }
    Ok((start_byte, end_byte))
}

fn validate_selection_replacement_document_size(
    text: &str,
    selection_start_byte: usize,
    selection_end_byte: usize,
    replacement: &str,
) -> Result<(), AppError> {
    let text_byte_len = editor_document_byte_len_from_usize(text.len())?;
    let selection_byte_len = editor_document_byte_len_from_usize(
        selection_end_byte.saturating_sub(selection_start_byte),
    )?;
    let replacement_byte_len = editor_document_byte_len_from_usize(replacement.len())?;
    let replaced_byte_len = checked_selection_replacement_result_byte_len(
        text_byte_len,
        selection_byte_len,
        replacement_byte_len,
    )?;
    validate_editor_document_byte_len(replaced_byte_len)
}

fn checked_selection_replacement_result_byte_len(
    text_byte_len: u64,
    selection_byte_len: u64,
    replacement_byte_len: u64,
) -> Result<u64, AppError> {
    text_byte_len
        .checked_sub(selection_byte_len)
        .and_then(|remaining| remaining.checked_add(replacement_byte_len))
        .ok_or(AppError::InvalidState(
            REPLACE_ALL_RESULT_TEXT_LENGTH_OVERFLOW_MESSAGE,
        ))
}

fn editor_document_byte_len_from_usize(byte_len: usize) -> Result<u64, AppError> {
    u64::try_from(byte_len).map_err(|_| AppError::InvalidState("Text is too large."))
}

fn validate_editor_document_byte_len(byte_len: u64) -> Result<(), AppError> {
    if !can_load_document_bytes(byte_len) {
        return Err(AppError::InvalidState(
            EDITOR_DOCUMENT_TEXT_TOO_LARGE_MESSAGE,
        ));
    }
    Ok(())
}

fn validate_plain_text_control_text(text: &str) -> Result<(), AppError> {
    if text.contains('\0') {
        return Err(plain_text_control_nul_error());
    }
    Ok(())
}

fn validate_plain_text_control_text_and_count_chars(text: &str) -> Result<usize, AppError> {
    let mut char_count = 0usize;
    for byte in text.bytes() {
        if byte == 0 {
            return Err(plain_text_control_nul_error());
        }
        if (byte & 0b1100_0000) != 0b1000_0000 {
            char_count += 1;
        }
    }
    Ok(char_count)
}

fn plain_text_control_nul_error() -> AppError {
    AppError::encoding_unsafe_text("Text has NUL and cannot be shown safely.")
}

fn get_window_text(hwnd: HWND) -> Result<String, AppError> {
    const MAX_ATTEMPTS: usize = 3;

    for _ in 0..MAX_ATTEMPTS {
        let length = get_window_text_length(hwnd, "get text length")?;
        if length == i32::MAX as usize {
            return Err(AppError::InvalidState("Text is too large."));
        }

        let mut buffer = vec![0u16; length + 1];
        unsafe {
            SetLastError(ERROR_SUCCESS);
        }
        let copied = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
        if copied < 0 {
            return Err(last_win32_error("get window text"));
        }
        if copied == 0 && unsafe { GetLastError() } != ERROR_SUCCESS {
            return Err(last_win32_error("get window text"));
        }

        let copied = copied as usize;
        if copied > length {
            return Err(AppError::InvalidState("Text is too large."));
        }
        if get_window_text_length(hwnd, "verify text length")? > copied {
            continue;
        }

        buffer.truncate(copied);
        return String::from_utf16(&buffer)
            .map_err(|_| AppError::encoding_decode("Window text contains invalid UTF-16"));
    }

    Err(AppError::InvalidState("Text changed while reading."))
}

fn get_window_text_length(hwnd: HWND, context: &'static str) -> Result<usize, AppError> {
    unsafe {
        SetLastError(ERROR_SUCCESS);
    }
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length < 0 {
        return Err(last_win32_error(context));
    }
    if length == 0 && unsafe { GetLastError() } != ERROR_SUCCESS {
        return Err(last_win32_error(context));
    }
    Ok(length as usize)
}

fn set_window_text(hwnd: HWND, text: &str) -> Result<(), AppError> {
    if text.contains('\0') {
        return Err(AppError::encoding_unsafe_text(
            "Text has NUL and cannot be shown safely.",
        ));
    }
    let text = wide_null(text);
    let ok = unsafe { SetWindowTextW(hwnd, text.as_ptr()) };
    if ok == 0 {
        return Err(last_win32_error("set window text"));
    }
    Ok(())
}

fn edit_selection(hwnd: HWND) -> (u32, u32) {
    let mut start = 0u32;
    let mut end = 0u32;
    unsafe {
        SendMessageW(
            hwnd,
            EM_GETSEL,
            (&mut start as *mut u32) as WPARAM,
            (&mut end as *mut u32) as LPARAM,
        );
    }
    (start, end)
}

fn edit_line_scroll_delta(current: usize, desired: usize) -> LPARAM {
    if desired >= current {
        desired.saturating_sub(current).min(isize::MAX as usize) as LPARAM
    } else {
        let distance = current.saturating_sub(desired).min(isize::MAX as usize) as isize;
        (-distance) as LPARAM
    }
}

fn set_edit_selection(hwnd: HWND, start: usize, end: usize) {
    unsafe {
        SendMessageW(hwnd, EM_SETSEL, start, end as LPARAM);
        SendMessageW(hwnd, EM_SCROLLCARET, 0, 0);
        SetFocus(hwnd);
    }
}

#[cfg(test)]
fn rich_edit_offset_to_text_offset(text: &str, rich_offset: usize) -> usize {
    map_text_offsets(text, rich_offset, OffsetMappingDirection::RichEditToText)
}

#[cfg(test)]
fn rich_edit_offset_to_byte_index(text: &str, rich_offset: usize) -> usize {
    let mut rich_units = 0usize;
    let mut chars = text.char_indices().peekable();

    while let Some((byte_index, ch)) = chars.next() {
        if rich_units >= rich_offset {
            return byte_index;
        }

        let (rich_step, byte_end) = match ch {
            '\r' if matches!(chars.peek(), Some((_, '\n'))) => match chars.next() {
                Some((line_feed_index, line_feed)) => {
                    (1, line_feed_index.saturating_add(line_feed.len_utf8()))
                }
                None => (1, byte_index.saturating_add(ch.len_utf8())),
            },
            '\r' | '\n' => (1, byte_index.saturating_add(ch.len_utf8())),
            _ => (ch.len_utf16(), byte_index.saturating_add(ch.len_utf8())),
        };

        if rich_units.saturating_add(rich_step) > rich_offset {
            return byte_index;
        }

        rich_units = rich_units.saturating_add(rich_step);
        if rich_units == rich_offset {
            return byte_end;
        }
    }

    text.len()
}

fn rich_edit_offset_range_to_byte_indices(
    text: &str,
    start_offset: usize,
    end_offset: usize,
) -> (usize, usize) {
    let (first_target, second_target, reversed) = if start_offset <= end_offset {
        (start_offset, end_offset, false)
    } else {
        (end_offset, start_offset, true)
    };
    let mut first_index = None;
    let mut second_index = None;
    let mut rich_units = 0usize;
    let mut chars = text.char_indices().peekable();

    while let Some((byte_index, ch)) = chars.next() {
        if first_index.is_none() && rich_units >= first_target {
            first_index = Some(byte_index);
        }
        if second_index.is_none() && rich_units >= second_target {
            second_index = Some(byte_index);
        }
        if first_index.is_some() && second_index.is_some() {
            break;
        }

        let (rich_step, byte_end) = match ch {
            '\r' if matches!(chars.peek(), Some((_, '\n'))) => match chars.next() {
                Some((line_feed_index, line_feed)) => {
                    (1, line_feed_index.saturating_add(line_feed.len_utf8()))
                }
                None => (1, byte_index.saturating_add(ch.len_utf8())),
            },
            '\r' | '\n' => (1, byte_index.saturating_add(ch.len_utf8())),
            _ => (ch.len_utf16(), byte_index.saturating_add(ch.len_utf8())),
        };
        let next_rich_units = rich_units.saturating_add(rich_step);

        if first_index.is_none() {
            if next_rich_units > first_target {
                first_index = Some(byte_index);
            } else if next_rich_units == first_target {
                first_index = Some(byte_end);
            }
        }
        if second_index.is_none() {
            if next_rich_units > second_target {
                second_index = Some(byte_index);
            } else if next_rich_units == second_target {
                second_index = Some(byte_end);
            }
        }

        rich_units = next_rich_units;
        if first_index.is_some() && second_index.is_some() {
            break;
        }
    }

    let first_index = match first_index {
        Some(index) => index,
        None => text.len(),
    };
    let second_index = match second_index {
        Some(index) => index,
        None => text.len(),
    };

    if reversed {
        (second_index, first_index)
    } else {
        (first_index, second_index)
    }
}

fn find_text_rich_edit_offsets_cached(
    cache: &mut Option<RichEditSearchOffsetCache>,
    document_id: DocumentId,
    content: Arc<str>,
    query: &str,
    rich_start_offset: usize,
    direction: SearchDirection,
) -> Option<(usize, usize)> {
    let should_reset = match cache.as_ref() {
        Some(previous) => !previous.matches(document_id, &content),
        None => true,
    };
    if should_reset {
        *cache = None;
    }

    let text = content.as_ref();
    let cache = cache
        .get_or_insert_with(|| RichEditSearchOffsetCache::new(document_id, Arc::clone(&content)));
    let start_checkpoint = cache.start_checkpoint(text, rich_start_offset);
    find_text(text, query, start_checkpoint.byte_index, direction).map(|range| {
        cache.byte_range_to_rich_edit_offsets(text, range.start, range.end, start_checkpoint)
    })
}

fn search_result_rich_edit_offsets_cached(
    cache: &mut Option<RichEditSearchOffsetCache>,
    document_id: DocumentId,
    content: Arc<str>,
    start_byte: usize,
    end_byte: usize,
) -> (usize, usize) {
    let should_reset = match cache.as_ref() {
        Some(previous) => !previous.matches(document_id, &content),
        None => true,
    };
    if should_reset {
        *cache = None;
    }

    let text = content.as_ref();
    cache
        .get_or_insert_with(|| RichEditSearchOffsetCache::new(document_id, Arc::clone(&content)))
        .byte_range_to_rich_edit_offsets_cached(text, start_byte, end_byte)
}

#[derive(Clone, Copy)]
struct RichEditSearchOffsetCheckpoint {
    byte_index: usize,
    rich_units: usize,
}

impl RichEditSearchOffsetCheckpoint {
    fn zero() -> Self {
        Self {
            byte_index: 0,
            rich_units: 0,
        }
    }
}

fn rich_edit_search_checkpoint_from_prefix(
    text: &str,
    rich_offset: usize,
    prefix_checkpoints: &mut Vec<RichEditSearchOffsetCheckpoint>,
) -> RichEditSearchOffsetCheckpoint {
    if prefix_checkpoints.is_empty() {
        prefix_checkpoints.push(RichEditSearchOffsetCheckpoint::zero());
    }

    let mut checkpoint = nearest_search_prefix_checkpoint_by_rich(prefix_checkpoints, rich_offset);
    let suffix = match text.get(checkpoint.byte_index..) {
        Some(suffix) => suffix,
        None => {
            prefix_checkpoints.clear();
            prefix_checkpoints.push(RichEditSearchOffsetCheckpoint::zero());
            checkpoint = RichEditSearchOffsetCheckpoint::zero();
            text
        }
    };
    let base_byte_index = checkpoint.byte_index;
    let mut next_checkpoint_rich = next_search_offset_checkpoint(checkpoint.rich_units);
    let mut chars = suffix.char_indices().peekable();

    while let Some((relative_byte_index, ch)) = chars.next() {
        if checkpoint.rich_units >= rich_offset {
            break;
        }

        let byte_index = base_byte_index.saturating_add(relative_byte_index);
        let (rich_step, byte_end) = match ch {
            '\r' if matches!(chars.peek(), Some((_, '\n'))) => match chars.next() {
                Some((line_feed_index, line_feed)) => (
                    1,
                    base_byte_index
                        .saturating_add(line_feed_index)
                        .saturating_add(line_feed.len_utf8()),
                ),
                None => (1, byte_index.saturating_add(ch.len_utf8())),
            },
            '\r' | '\n' => (1, byte_index.saturating_add(ch.len_utf8())),
            _ => (ch.len_utf16(), byte_index.saturating_add(ch.len_utf8())),
        };
        let next_rich_units = checkpoint.rich_units.saturating_add(rich_step);

        if next_rich_units > rich_offset {
            return RichEditSearchOffsetCheckpoint {
                byte_index,
                rich_units: checkpoint.rich_units,
            };
        }

        checkpoint = RichEditSearchOffsetCheckpoint {
            byte_index: byte_end,
            rich_units: next_rich_units,
        };

        while checkpoint.rich_units >= next_checkpoint_rich {
            record_search_prefix_checkpoint(prefix_checkpoints, checkpoint);
            let next = next_checkpoint_rich.saturating_add(SEARCH_OFFSET_CHECKPOINT_RICH_UNITS);
            if next == next_checkpoint_rich {
                break;
            }
            next_checkpoint_rich = next;
        }
    }

    checkpoint
}

fn nearest_search_prefix_checkpoint_by_rich(
    prefix_checkpoints: &[RichEditSearchOffsetCheckpoint],
    rich_offset: usize,
) -> RichEditSearchOffsetCheckpoint {
    match prefix_checkpoints.binary_search_by_key(&rich_offset, |checkpoint| checkpoint.rich_units)
    {
        Ok(index) => prefix_checkpoints[index],
        Err(0) => RichEditSearchOffsetCheckpoint::zero(),
        Err(index) => prefix_checkpoints[index - 1],
    }
}

fn nearest_search_prefix_checkpoint_by_byte(
    prefix_checkpoints: &[RichEditSearchOffsetCheckpoint],
    byte_index: usize,
) -> RichEditSearchOffsetCheckpoint {
    match prefix_checkpoints.binary_search_by_key(&byte_index, |checkpoint| checkpoint.byte_index) {
        Ok(index) => prefix_checkpoints[index],
        Err(0) => RichEditSearchOffsetCheckpoint::zero(),
        Err(index) => prefix_checkpoints[index - 1],
    }
}

fn record_search_prefix_checkpoint(
    prefix_checkpoints: &mut Vec<RichEditSearchOffsetCheckpoint>,
    checkpoint: RichEditSearchOffsetCheckpoint,
) {
    if checkpoint.rich_units == 0 {
        return;
    }

    match prefix_checkpoints.binary_search_by_key(&checkpoint.rich_units, |item| item.rich_units) {
        Ok(_) => {}
        Err(index) => prefix_checkpoints.insert(index, checkpoint),
    }
}

fn byte_range_to_rich_edit_offsets_with_prefix(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    start_checkpoint: RichEditSearchOffsetCheckpoint,
    prefix_checkpoints: &[RichEditSearchOffsetCheckpoint],
) -> (usize, usize) {
    let start_byte = floor_byte_index(text, start_byte);
    let checkpoint = if start_checkpoint.byte_index <= start_byte {
        start_checkpoint
    } else {
        nearest_search_prefix_checkpoint_by_byte(prefix_checkpoints, start_byte)
    };

    byte_range_to_rich_edit_offsets_from(text, start_byte, end_byte, checkpoint)
}

fn byte_range_to_rich_edit_offsets_with_cached_prefix(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    prefix_checkpoints: &mut Vec<RichEditSearchOffsetCheckpoint>,
) -> (usize, usize) {
    if prefix_checkpoints.is_empty() {
        prefix_checkpoints.push(RichEditSearchOffsetCheckpoint::zero());
    }

    let start_byte = floor_byte_index(text, start_byte);
    let checkpoint = nearest_search_prefix_checkpoint_by_byte(prefix_checkpoints, start_byte);
    byte_range_to_rich_edit_offsets_from_with_cache(
        text,
        start_byte,
        end_byte,
        checkpoint,
        prefix_checkpoints,
    )
}

fn next_search_offset_checkpoint(rich_offset: usize) -> usize {
    rich_offset.saturating_add(
        SEARCH_OFFSET_CHECKPOINT_RICH_UNITS - rich_offset % SEARCH_OFFSET_CHECKPOINT_RICH_UNITS,
    )
}

#[cfg(test)]
fn text_offset_to_rich_edit_offset(text: &str, text_offset: usize) -> usize {
    map_text_offsets(text, text_offset, OffsetMappingDirection::TextToRichEdit)
}

#[cfg(test)]
fn text_offset_range_to_rich_edit_offsets(
    text: &str,
    start_offset: usize,
    end_offset: usize,
) -> (usize, usize) {
    map_text_offset_range(
        text,
        start_offset,
        end_offset,
        OffsetMappingDirection::TextToRichEdit,
    )
}

#[cfg(test)]
fn byte_range_to_rich_edit_offsets(
    text: &str,
    start_byte: usize,
    end_byte: usize,
) -> (usize, usize) {
    byte_range_to_rich_edit_offsets_from(
        text,
        start_byte,
        end_byte,
        RichEditSearchOffsetCheckpoint::zero(),
    )
}

fn byte_range_to_rich_edit_offsets_from(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    checkpoint: RichEditSearchOffsetCheckpoint,
) -> (usize, usize) {
    byte_range_to_rich_edit_offsets_from_inner(text, start_byte, end_byte, checkpoint, None)
}

fn byte_range_to_rich_edit_offsets_from_with_cache(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    checkpoint: RichEditSearchOffsetCheckpoint,
    prefix_checkpoints: &mut Vec<RichEditSearchOffsetCheckpoint>,
) -> (usize, usize) {
    byte_range_to_rich_edit_offsets_from_inner(
        text,
        start_byte,
        end_byte,
        checkpoint,
        Some(prefix_checkpoints),
    )
}

struct RichEditSearchOffsetCheckpointRecorder<'a> {
    prefix_checkpoints: Option<&'a mut Vec<RichEditSearchOffsetCheckpoint>>,
    next_checkpoint_rich: usize,
}

impl<'a> RichEditSearchOffsetCheckpointRecorder<'a> {
    fn new(
        prefix_checkpoints: Option<&'a mut Vec<RichEditSearchOffsetCheckpoint>>,
        rich_units: usize,
    ) -> Self {
        Self {
            prefix_checkpoints,
            next_checkpoint_rich: next_search_offset_checkpoint(rich_units),
        }
    }

    fn reset(&mut self, rich_units: usize) {
        if let Some(prefix_checkpoints) = self.prefix_checkpoints.as_mut() {
            let prefix_checkpoints = &mut **prefix_checkpoints;
            prefix_checkpoints.clear();
            prefix_checkpoints.push(RichEditSearchOffsetCheckpoint::zero());
        }
        self.next_checkpoint_rich = next_search_offset_checkpoint(rich_units);
    }

    fn record(&mut self, checkpoint: RichEditSearchOffsetCheckpoint) {
        let Some(prefix_checkpoints) = self.prefix_checkpoints.as_mut() else {
            return;
        };
        let prefix_checkpoints = &mut **prefix_checkpoints;
        while checkpoint.rich_units >= self.next_checkpoint_rich {
            record_search_prefix_checkpoint(prefix_checkpoints, checkpoint);
            let next = self
                .next_checkpoint_rich
                .saturating_add(SEARCH_OFFSET_CHECKPOINT_RICH_UNITS);
            if next == self.next_checkpoint_rich {
                break;
            }
            self.next_checkpoint_rich = next;
        }
    }
}

fn byte_range_to_rich_edit_offsets_from_inner(
    text: &str,
    start_byte: usize,
    end_byte: usize,
    mut checkpoint: RichEditSearchOffsetCheckpoint,
    prefix_checkpoints: Option<&mut Vec<RichEditSearchOffsetCheckpoint>>,
) -> (usize, usize) {
    let start_byte = floor_byte_index(text, start_byte);
    let end_byte = floor_byte_index(text, end_byte);
    let mut recorder =
        RichEditSearchOffsetCheckpointRecorder::new(prefix_checkpoints, checkpoint.rich_units);
    let suffix = match text.get(checkpoint.byte_index..) {
        Some(suffix) => suffix,
        None => {
            checkpoint = RichEditSearchOffsetCheckpoint::zero();
            recorder.reset(checkpoint.rich_units);
            text
        }
    };
    let mut start_offset = (start_byte <= checkpoint.byte_index).then_some(checkpoint.rich_units);
    let mut end_offset = (end_byte <= checkpoint.byte_index).then_some(checkpoint.rich_units);
    let mut rich_units = checkpoint.rich_units;
    let mut chars = suffix.char_indices().peekable();

    while let Some((byte_index, ch)) = chars.next() {
        let byte_index = checkpoint.byte_index.saturating_add(byte_index);
        if start_offset.is_none() && start_byte <= byte_index {
            start_offset = Some(rich_units);
        }
        if end_offset.is_none() && end_byte <= byte_index {
            end_offset = Some(rich_units);
        }
        if start_offset.is_some() && end_offset.is_some() {
            break;
        }

        let (rich_step, segment_end, crlf_segment) = match ch {
            '\r' if matches!(chars.peek(), Some((_, '\n'))) => {
                let segment_end = match chars.next() {
                    Some((line_feed_index, line_feed)) => checkpoint
                        .byte_index
                        .saturating_add(line_feed_index)
                        .saturating_add(line_feed.len_utf8()),
                    None => byte_index.saturating_add(ch.len_utf8()),
                };
                (1, segment_end, true)
            }
            '\r' | '\n' => (1, byte_index.saturating_add(ch.len_utf8()), false),
            _ => (
                ch.len_utf16(),
                byte_index.saturating_add(ch.len_utf8()),
                false,
            ),
        };
        let next_rich_units = rich_units.saturating_add(rich_step);

        if crlf_segment {
            if start_offset.is_none() && start_byte < segment_end {
                start_offset = Some(next_rich_units);
            }
            if end_offset.is_none() && end_byte < segment_end {
                end_offset = Some(next_rich_units);
            }
        }

        rich_units = next_rich_units;
        recorder.record(RichEditSearchOffsetCheckpoint {
            byte_index: segment_end,
            rich_units,
        });
    }

    let start_offset = match start_offset {
        Some(offset) => offset,
        None => rich_units,
    };
    let end_offset = match end_offset {
        Some(offset) => offset,
        None => rich_units,
    };

    (start_offset, end_offset)
}

const SEARCH_OFFSET_CHECKPOINT_RICH_UNITS: usize = 4096;

fn floor_byte_index(text: &str, byte_index: usize) -> usize {
    let mut index = byte_index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
#[derive(Clone, Copy)]
enum OffsetMappingDirection {
    #[cfg(test)]
    RichEditToText,
    TextToRichEdit,
}

#[cfg(test)]
fn map_text_offsets(text: &str, target: usize, direction: OffsetMappingDirection) -> usize {
    let mut text_units = 0usize;
    let mut rich_units = 0usize;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        let (text_step, rich_step) = match ch {
            '\r' if matches!(chars.peek(), Some('\n')) => {
                chars.next();
                (2, 1)
            }
            '\r' | '\n' => (1, 1),
            _ => {
                let width = ch.len_utf16();
                (width, width)
            }
        };

        let (source_units, source_step, target_units, target_step) = match direction {
            OffsetMappingDirection::RichEditToText => {
                (rich_units, rich_step, text_units, text_step)
            }
            OffsetMappingDirection::TextToRichEdit => {
                (text_units, text_step, rich_units, rich_step)
            }
        };

        if source_units >= target {
            break;
        }
        if source_units + source_step > target {
            return target_units + target.saturating_sub(source_units).min(target_step);
        }

        text_units = text_units.saturating_add(text_step);
        rich_units = rich_units.saturating_add(rich_step);
    }

    match direction {
        OffsetMappingDirection::RichEditToText => text_units,
        OffsetMappingDirection::TextToRichEdit => rich_units,
    }
}

#[cfg(test)]
fn map_text_offset_range(
    text: &str,
    start_target: usize,
    end_target: usize,
    direction: OffsetMappingDirection,
) -> (usize, usize) {
    let (first_target, second_target, reversed) = if start_target <= end_target {
        (start_target, end_target, false)
    } else {
        (end_target, start_target, true)
    };
    let mut first_offset = None;
    let mut second_offset = None;
    let mut text_units = 0usize;
    let mut rich_units = 0usize;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        let (source_units, target_units) = match direction {
            #[cfg(test)]
            OffsetMappingDirection::RichEditToText => (rich_units, text_units),
            OffsetMappingDirection::TextToRichEdit => (text_units, rich_units),
        };
        if first_offset.is_none() && source_units >= first_target {
            first_offset = Some(target_units);
        }
        if second_offset.is_none() && source_units >= second_target {
            second_offset = Some(target_units);
        }
        if first_offset.is_some() && second_offset.is_some() {
            break;
        }

        let (text_step, rich_step) = match ch {
            '\r' if matches!(chars.peek(), Some('\n')) => {
                chars.next();
                (2, 1)
            }
            '\r' | '\n' => (1, 1),
            _ => {
                let width = ch.len_utf16();
                (width, width)
            }
        };
        let (source_step, target_step) = match direction {
            #[cfg(test)]
            OffsetMappingDirection::RichEditToText => (rich_step, text_step),
            OffsetMappingDirection::TextToRichEdit => (text_step, rich_step),
        };
        let next_source_units = source_units.saturating_add(source_step);

        if first_offset.is_none() && next_source_units > first_target {
            first_offset = Some(
                target_units
                    .saturating_add(first_target.saturating_sub(source_units).min(target_step)),
            );
        }
        if second_offset.is_none() && next_source_units > second_target {
            second_offset = Some(
                target_units
                    .saturating_add(second_target.saturating_sub(source_units).min(target_step)),
            );
        }

        text_units = text_units.saturating_add(text_step);
        rich_units = rich_units.saturating_add(rich_step);
        let (source_units, target_units) = match direction {
            #[cfg(test)]
            OffsetMappingDirection::RichEditToText => (rich_units, text_units),
            OffsetMappingDirection::TextToRichEdit => (text_units, rich_units),
        };
        if first_offset.is_none() && source_units >= first_target {
            first_offset = Some(target_units);
        }
        if second_offset.is_none() && source_units >= second_target {
            second_offset = Some(target_units);
        }
        if first_offset.is_some() && second_offset.is_some() {
            break;
        }
    }

    let final_target_units = match direction {
        #[cfg(test)]
        OffsetMappingDirection::RichEditToText => text_units,
        OffsetMappingDirection::TextToRichEdit => rich_units,
    };
    let first_offset = match first_offset {
        Some(offset) => offset,
        None => final_target_units,
    };
    let second_offset = match second_offset {
        Some(offset) => offset,
        None => final_target_units,
    };

    if reversed {
        (second_offset, first_offset)
    } else {
        (first_offset, second_offset)
    }
}

#[cfg(test)]
fn selected_char_count_from_rich_edit_offsets(
    text: &str,
    rich_selection_start: usize,
    rich_selection_end: usize,
) -> usize {
    if rich_selection_start == rich_selection_end {
        return 0;
    }

    let selection_start = rich_selection_start.min(rich_selection_end);
    let selection_end = rich_selection_start.max(rich_selection_end);
    let mut rich_units = 0usize;
    let mut selected_chars = 0usize;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        let (rich_step, char_count) = match ch {
            '\r' if matches!(chars.peek(), Some('\n')) => {
                chars.next();
                (1, 2)
            }
            '\r' | '\n' => (1, 1),
            _ => (ch.len_utf16(), 1),
        };
        let next_rich_units = rich_units.saturating_add(rich_step);

        if selection_start < next_rich_units && selection_end >= next_rich_units {
            selected_chars = selected_chars.saturating_add(char_count);
        }

        rich_units = next_rich_units;
        if rich_units >= selection_end {
            break;
        }
    }

    selected_chars
}

const SELECTION_METRICS_CHECKPOINT_RICH_UNITS: usize = 4096;

fn selected_char_count_cached(
    cache: &mut Option<SelectionMetricsCache>,
    document_id: DocumentId,
    text: &str,
    rich_selection_start: u32,
    rich_selection_end: u32,
) -> usize {
    let should_reset = match cache.as_ref() {
        Some(previous) => previous.document_id != document_id || previous.text_len != text.len(),
        None => true,
    };
    if should_reset {
        *cache = None;
    }
    let cache = cache.get_or_insert_with(|| SelectionMetricsCache::new(document_id, text.len()));
    let raw_selection_start = rich_selection_start;
    let raw_selection_end = rich_selection_end;
    let rich_selection_start = raw_selection_start.min(raw_selection_end);
    let rich_selection_end = raw_selection_start.max(raw_selection_end);

    if rich_selection_start == rich_selection_end {
        cache.rich_selection_start = rich_selection_start;
        cache.rich_selection_end = rich_selection_end;
        cache.selected_chars = 0;
        return 0;
    }

    if cache.rich_selection_start == rich_selection_start
        && cache.rich_selection_end == rich_selection_end
    {
        return cache.selected_chars;
    }

    let selection_start = selected_prefix_metric_cached(cache, text, rich_selection_start as usize);
    cache.record_recent_prefix_metric(0, selection_start);
    let selection_end = selected_prefix_metric_cached(cache, text, rich_selection_end as usize);
    cache.record_recent_prefix_metric(1, selection_end);
    let selected_chars = selection_end
        .char_count
        .saturating_sub(selection_start.char_count);
    cache.rich_selection_start = rich_selection_start;
    cache.rich_selection_end = rich_selection_end;
    cache.selected_chars = selected_chars;
    selected_chars
}

fn selected_prefix_metric_cached(
    cache: &mut SelectionMetricsCache,
    text: &str,
    rich_offset: usize,
) -> SelectionPrefixMetric {
    let checkpoint = cache.nearest_prefix_checkpoint(rich_offset);
    let mut metric = checkpoint;
    let suffix = match text.get(metric.byte_index..) {
        Some(suffix) => suffix,
        None => {
            cache.reset_prefix_metrics();
            metric = SelectionPrefixMetric::zero();
            text
        }
    };
    let base_byte_index = metric.byte_index;
    let mut next_checkpoint_rich = next_selection_metrics_checkpoint(metric.rich_offset);
    let mut chars = suffix.char_indices().peekable();

    while let Some((byte_index, ch)) = chars.next() {
        if metric.rich_offset >= rich_offset {
            break;
        }

        let (rich_step, char_count, segment_end_byte) = match ch {
            '\r' if matches!(chars.peek(), Some((_, '\n'))) => match chars.next() {
                Some((line_feed_index, line_feed)) => (
                    1,
                    2,
                    base_byte_index
                        .saturating_add(line_feed_index)
                        .saturating_add(line_feed.len_utf8()),
                ),
                None => (
                    1,
                    1,
                    base_byte_index
                        .saturating_add(byte_index)
                        .saturating_add(ch.len_utf8()),
                ),
            },
            '\r' | '\n' => (
                1,
                1,
                base_byte_index
                    .saturating_add(byte_index)
                    .saturating_add(ch.len_utf8()),
            ),
            _ => (
                ch.len_utf16(),
                1,
                base_byte_index
                    .saturating_add(byte_index)
                    .saturating_add(ch.len_utf8()),
            ),
        };
        let next_rich_offset = metric.rich_offset.saturating_add(rich_step);
        if next_rich_offset > rich_offset {
            break;
        }

        metric = SelectionPrefixMetric {
            rich_offset: next_rich_offset,
            byte_index: segment_end_byte,
            char_count: metric.char_count.saturating_add(char_count),
        };

        while metric.rich_offset >= next_checkpoint_rich {
            cache.record_prefix_checkpoint(metric);
            let next = next_checkpoint_rich.saturating_add(SELECTION_METRICS_CHECKPOINT_RICH_UNITS);
            if next == next_checkpoint_rich {
                break;
            }
            next_checkpoint_rich = next;
        }
    }

    metric
}

fn next_selection_metrics_checkpoint(rich_offset: usize) -> usize {
    rich_offset.saturating_add(
        SELECTION_METRICS_CHECKPOINT_RICH_UNITS
            - rich_offset % SELECTION_METRICS_CHECKPOINT_RICH_UNITS,
    )
}

fn show_error(owner: HWND, error: &AppError) {
    let text = error.user_message();
    message_box(owner, &text, "j3Text Error", MB_OK | MB_ICONERROR);
}

fn show_about_dialog(owner: HWND) {
    if let Err(error) = show_about_custom_dialog(owner) {
        show_error(owner, &error);
    }
}

fn about_dialog_body_text() -> Cow<'static, str> {
    about_text()
}

fn show_about_custom_dialog(owner: HWND) -> Result<(), AppError> {
    init_rich_edit()?;
    let scale = DpiMetrics::for_window(owner).ui_scale();
    let window = create_about_dialog_window(owner, scale)?;
    let _window_guard = WindowDestroyGuard::new(window);

    let _message_label = create_dialog_control(
        window,
        DialogControlSpec {
            class_name: "STATIC",
            text: ABOUT_DIALOG_VERSION_LABEL,
            bounds: scale_dialog_bounds(scale, (16, 14, ABOUT_BODY_SCROLL_WIDTH, 24)),
            style: SS_CENTERIMAGE,
            ex_style: 0,
            id: 0,
        },
    )?;
    let body_edit = create_dialog_control(
        window,
        DialogControlSpec {
            class_name: RICH_EDIT_CLASS,
            text: "",
            bounds: scale_dialog_bounds(
                scale,
                (16, 48, ABOUT_BODY_SCROLL_WIDTH, ABOUT_BODY_SCROLL_HEIGHT),
            ),
            style: ES_LEFT
                | ES_MULTILINE
                | ES_AUTOVSCROLL
                | ES_AUTOHSCROLL
                | ES_NOHIDESEL
                | WS_VSCROLL
                | WS_HSCROLL
                | WS_TABSTOP,
            ex_style: WS_EX_CLIENTEDGE,
            id: 0,
        },
    )?;
    let open_url_button = create_dialog_control(
        window,
        DialogControlSpec {
            class_name: "BUTTON",
            text: ABOUT_DIALOG_URL,
            bounds: scale_dialog_bounds(scale, (16, ABOUT_BUTTON_TOP, 300, 28)),
            style: BS_PUSHBUTTON as u32 | WS_TABSTOP,
            ex_style: 0,
            id: ID_ABOUT_OPEN_URL,
        },
    )?;
    let ok_button = create_dialog_control(
        window,
        DialogControlSpec {
            class_name: "BUTTON",
            text: "OK",
            bounds: scale_dialog_bounds(scale, (330, ABOUT_BUTTON_TOP, 88, 28)),
            style: BS_PUSHBUTTON as u32 | WS_TABSTOP,
            ex_style: 0,
            id: ID_ABOUT_OK,
        },
    )?;

    let body = EditorTextSurface::from_hwnd(body_edit)
        .ok_or(AppError::InvalidState("About text surface was not created"))?;
    body.initialize_plain_text()?;
    body.set_text_limit(RICH_EDIT_SURFACE_TEXT_LIMIT);
    let body_text = about_dialog_body_text();
    body.set_text(body_text.as_ref())?;
    body.set_readonly(true)?;
    body.set_modified(false);
    apply_about_body_font(body_edit);

    center_window_over_owner(window, owner);
    let _modal_guard = CustomModalDialogGuard::install(owner);
    unsafe {
        ShowWindow(window, SW_SHOW);
        SetFocus(ok_button);
    }

    run_about_dialog_loop(window, ok_button, open_url_button)
}

fn create_about_dialog_window(owner: HWND, scale: UiScale) -> Result<HWND, AppError> {
    let instance = unsafe { GetModuleHandleW(null()) };
    if instance.is_null() {
        return Err(last_win32_error("get module handle"));
    }
    register_about_dialog_class(instance)?;

    let class = wide_null(ABOUT_DIALOG_CLASS_NAME);
    let title = wide_null(ABOUT_DIALOG_TITLE);
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            ABOUT_DIALOG_STYLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale.px(ABOUT_DIALOG_WIDTH),
            scale.px(ABOUT_DIALOG_HEIGHT),
            owner,
            null_mut(),
            instance,
            null_mut(),
        )
    };
    if window.is_null() {
        return Err(last_win32_error("create About dialog window"));
    }
    Ok(window)
}

fn register_about_dialog_class(instance: HINSTANCE) -> Result<(), AppError> {
    const ERROR_CLASS_ALREADY_EXISTS_LOCAL: u32 = 1410;
    static REGISTER: OnceLock<Result<(), u32>> = OnceLock::new();

    match REGISTER.get_or_init(|| {
        let class_name = wide_null(ABOUT_DIALOG_CLASS_NAME);
        let cursor = unsafe { LoadCursorW(null_mut(), IDC_ARROW) };
        if cursor.is_null() {
            return Err(unsafe { GetLastError() });
        }
        let icon = match load_app_icon(instance) {
            Ok(icon) => icon,
            Err(_) => null_mut(),
        };
        let wnd_class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            hCursor: cursor,
            hIcon: icon,
            hIconSm: icon,
            hbrBackground: unsafe { GetSysColorBrush(COLOR_BTNFACE) },
            ..unsafe { MaybeUninit::<WNDCLASSEXW>::zeroed().assume_init() }
        };

        let atom = unsafe { RegisterClassExW(&wnd_class) };
        if atom == 0 {
            let error = unsafe { GetLastError() };
            if error == ERROR_CLASS_ALREADY_EXISTS_LOCAL {
                Ok(())
            } else {
                Err(error)
            }
        } else {
            Ok(())
        }
    }) {
        Ok(()) => Ok(()),
        Err(code) => Err(AppError::win32("register About dialog window class", *code)),
    }
}

fn scale_dialog_bounds(scale: UiScale, bounds: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    let (x, y, width, height) = bounds;
    (scale.px(x), scale.px(y), scale.px(width), scale.px(height))
}

fn apply_about_body_font(hwnd: HWND) {
    let fixed_font = unsafe { GetStockObject(ANSI_FIXED_FONT) };
    if !fixed_font.is_null() {
        unsafe {
            SendMessageW(hwnd, WM_SETFONT_LOCAL, fixed_font as WPARAM, 1);
        }
    }
}

fn run_about_dialog_loop(
    window: HWND,
    ok_button: HWND,
    open_url_button: HWND,
) -> Result<(), AppError> {
    let mut message = unsafe { MaybeUninit::<MSG>::zeroed().assume_init() };
    while unsafe { IsWindow(window) != 0 } {
        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if result == -1 {
            unsafe {
                DestroyWindow(window);
            }
            return Err(last_win32_error("get About dialog message"));
        }
        if result == 0 {
            unsafe {
                DestroyWindow(window);
                PostQuitMessage(0);
            }
            break;
        }

        if message.hwnd == window
            && (message.message == WM_CLOSE
                || (message.message == WM_SYSCOMMAND
                    && (message.wParam & 0xfff0) == SC_CLOSE as WPARAM))
        {
            unsafe {
                DestroyWindow(window);
            }
            break;
        }

        if message.message == WM_COMMAND {
            match loword(message.wParam) {
                ID_ABOUT_OK => {
                    unsafe {
                        DestroyWindow(window);
                    }
                    break;
                }
                ID_ABOUT_OPEN_URL => {
                    open_about_dialog_url(window);
                    continue;
                }
                _ => {}
            }
        }

        if matches!(message.message, WM_KEYDOWN | WM_SYSKEYDOWN) {
            match message.wParam as u32 {
                VK_ESCAPE_CODE | VK_RETURN_CODE => {
                    unsafe {
                        DestroyWindow(window);
                    }
                    break;
                }
                _ => {}
            }
        }

        let ok_clicked = button_activated_by_message(ok_button, &message);
        let open_url_clicked = button_activated_by_message(open_url_button, &message);
        let handled = if about_dialog_should_preprocess_message(message.message) {
            unsafe { IsDialogMessageW(window, &message) }
        } else {
            0
        };
        if handled == 0 {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }

        if unsafe { IsWindow(window) == 0 } {
            break;
        }
        if ok_clicked {
            unsafe {
                DestroyWindow(window);
            }
            break;
        }
        if open_url_clicked {
            open_about_dialog_url(window);
        }
    }

    Ok(())
}

fn about_dialog_should_preprocess_message(message: u32) -> bool {
    matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN)
}

fn open_about_dialog_url(owner: HWND) {
    let url = wide_null(ABOUT_DIALOG_URL);
    let result =
        unsafe { ShellExecuteW(owner, null(), url.as_ptr(), null(), null(), SW_SHOWNORMAL) };
    if result as isize <= 32 {
        message_box(
            owner,
            &format!("Could not open project URL.\n\n{ABOUT_DIALOG_URL}"),
            ABOUT_DIALOG_TITLE,
            MB_OK | MB_ICONWARNING,
        );
    }
}

fn external_file_changed_action_dialog(owner: HWND, path: &Path) -> ExternalFileChangedAction {
    let title = wide_null("File Changed");
    let instruction = wide_null("File changed outside j3Text.");
    let content = wide_null(&format!(
        "{}\r\n\r\nReload discards this tab's current text and reads the file from disk.\r\nSave As keeps this tab's current text and saves it to another file.",
        path.display()
    ));
    let reload_text = wide_null("Reload");
    let save_as_text = wide_null("Save As");
    let cancel_text = wide_null("Cancel");
    let buttons = [
        TASKDIALOG_BUTTON {
            nButtonID: EXTERNAL_CHANGE_RELOAD_BUTTON,
            pszButtonText: reload_text.as_ptr(),
        },
        TASKDIALOG_BUTTON {
            nButtonID: EXTERNAL_CHANGE_SAVE_AS_BUTTON,
            pszButtonText: save_as_text.as_ptr(),
        },
        TASKDIALOG_BUTTON {
            nButtonID: EXTERNAL_CHANGE_CANCEL_BUTTON,
            pszButtonText: cancel_text.as_ptr(),
        },
    ];
    let mut config = TASKDIALOGCONFIG {
        cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
        hwndParent: owner,
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION,
        pszWindowTitle: title.as_ptr(),
        pszMainInstruction: instruction.as_ptr(),
        pszContent: content.as_ptr(),
        cButtons: buttons.len() as u32,
        pButtons: buttons.as_ptr(),
        nDefaultButton: EXTERNAL_CHANGE_CANCEL_BUTTON,
        ..TASKDIALOGCONFIG::default()
    };
    config.Anonymous1.pszMainIcon = TD_ERROR_ICON;

    let mut selected = EXTERNAL_CHANGE_CANCEL_BUTTON;
    let result = with_centered_dialog(owner, || unsafe {
        TaskDialogIndirect(&config, &mut selected, null_mut(), null_mut())
    });
    if result < 0 {
        return external_file_changed_fallback_dialog(owner, path);
    }
    external_file_changed_action_from_button(selected)
}

fn external_file_changed_fallback_dialog(owner: HWND, path: &Path) -> ExternalFileChangedAction {
    let message = format!(
        "{} changed outside j3Text.\n\nYes: Reload from disk\nNo: Save As\nCancel: keep this tab unchanged",
        path.display()
    );
    match message_box(
        owner,
        &message,
        "File Changed",
        MB_YESNOCANCEL | MB_ICONERROR,
    ) {
        IDYES => ExternalFileChangedAction::Reload,
        IDNO => ExternalFileChangedAction::SaveAs,
        _ => ExternalFileChangedAction::Cancel,
    }
}

fn external_file_changed_action_from_button(button: i32) -> ExternalFileChangedAction {
    match button {
        EXTERNAL_CHANGE_RELOAD_BUTTON => ExternalFileChangedAction::Reload,
        EXTERNAL_CHANGE_SAVE_AS_BUTTON => ExternalFileChangedAction::SaveAs,
        _ => ExternalFileChangedAction::Cancel,
    }
}

fn show_visible_whitespace_size_limit(owner: HWND, byte_len: usize) {
    let limit_mb = VISIBLE_WHITESPACE_RENDER_LIMIT_BYTES as f64 / (1024.0 * 1024.0);
    let document_mb = byte_len as f64 / (1024.0 * 1024.0);
    let message = format!(
        "Marks work up to {limit_mb:.0} MB.\n\nThis file is {document_mb:.1} MB, so normal view stays on."
    );
    message_box(owner, &message, "Marks", MB_OK | MB_ICONINFORMATION);
}

fn launch_document_in_new_window(path: &Path) -> Result<(), AppError> {
    let executable =
        env::current_exe().map_err(|error| AppError::io(error, "find app executable"))?;
    Command::new(&executable)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            AppError::io_path_with_user(
                error,
                "launch new window",
                executable,
                "open in new window",
                path.to_path_buf(),
            )
        })
}

fn message_box(owner: HWND, text: &str, title: &str, flags: u32) -> i32 {
    let text = wide_null(text);
    let title = wide_null(title);
    with_centered_dialog(owner, || unsafe {
        MessageBoxW(owner, text.as_ptr(), title.as_ptr(), flags)
    })
}

fn with_centered_dialog<T>(owner: HWND, show_dialog: impl FnOnce() -> T) -> T {
    let _modal_guard = ActiveModalDialogGuard::enter();
    let _guard = CenteredDialogGuard::install(owner);
    show_dialog()
}

fn modal_dialog_active() -> bool {
    ACTIVE_MODAL_DIALOG_DEPTH.with(|depth| depth.get() > 0)
}

struct ActiveModalDialogGuard;

impl ActiveModalDialogGuard {
    fn enter() -> Self {
        ACTIVE_MODAL_DIALOG_DEPTH.with(|depth| {
            depth.set(depth.get().saturating_add(1));
        });
        Self
    }
}

impl Drop for ActiveModalDialogGuard {
    fn drop(&mut self) {
        ACTIVE_MODAL_DIALOG_DEPTH.with(|depth| {
            depth.set(depth.get().saturating_sub(1));
        });
    }
}

struct CenteredDialogGuard {
    hook: HHOOK,
    previous_owner: HWND,
}

impl CenteredDialogGuard {
    fn install(owner: HWND) -> Self {
        let previous_owner = CENTERED_DIALOG_OWNER.with(|cell| {
            let previous = cell.get();
            cell.set(owner);
            previous
        });

        if owner.is_null() {
            return Self {
                hook: null_mut(),
                previous_owner,
            };
        }

        let thread_id = unsafe { GetCurrentThreadId() };
        let hook = unsafe {
            SetWindowsHookExW(
                WH_CBT,
                Some(centered_dialog_hook_proc),
                null_mut(),
                thread_id,
            )
        };
        Self {
            hook,
            previous_owner,
        }
    }
}

impl Drop for CenteredDialogGuard {
    fn drop(&mut self) {
        if !self.hook.is_null() {
            unsafe {
                let _ = UnhookWindowsHookEx(self.hook);
            }
        }
        CENTERED_DIALOG_OWNER.with(|cell| cell.set(self.previous_owner));
    }
}

unsafe extern "system" fn centered_dialog_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HCBT_ACTIVATE as i32 {
        CENTERED_DIALOG_OWNER.with(|cell| {
            center_window_over_owner(wparam as HWND, cell.get());
        });
    }

    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

fn center_window_over_owner(window: HWND, owner: HWND) {
    if window.is_null() || owner.is_null() || window == owner {
        return;
    }

    let mut owner_rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let mut window_rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };

    let got_rects = unsafe {
        GetWindowRect(owner, &mut owner_rect) != 0 && GetWindowRect(window, &mut window_rect) != 0
    };
    if !got_rects {
        return;
    }

    let owner_width = owner_rect.right - owner_rect.left;
    let owner_height = owner_rect.bottom - owner_rect.top;
    let window_width = window_rect.right - window_rect.left;
    let window_height = window_rect.bottom - window_rect.top;
    if owner_width <= 0 || owner_height <= 0 || window_width <= 0 || window_height <= 0 {
        return;
    }

    let x = owner_rect.left + (owner_width - window_width) / 2;
    let y = owner_rect.top + (owner_height - window_height) / 2;
    unsafe {
        let _ = SetWindowPos(
            window,
            null_mut(),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn move_window(hwnd: HWND, x: i32, y: i32, width: i32, height: i32, repaint: bool) {
    if hwnd.is_null() {
        return;
    }
    unsafe {
        MoveWindow(hwnd, x, y, width, height, repaint as i32);
    }
}

fn show_window(hwnd: HWND, show: bool) {
    if hwnd.is_null() {
        return;
    }
    unsafe {
        ShowWindow(hwnd, if show { SW_SHOW } else { SW_HIDE });
    }
}

fn apply_non_client_dark_mode(hwnd: HWND, dark: bool) {
    if hwnd.is_null() {
        return;
    }
    let enabled = i32::from(dark);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&enabled as *const i32).cast(),
            size_of::<i32>() as u32,
        );
    }
}

const DEFAULT_DPI: u32 = 96;
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE: DpiAwarenessContext = -3isize as _;
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: DpiAwarenessContext = -4isize as _;
const PROCESS_PER_MONITOR_DPI_AWARE: ProcessDpiAwareness = 2;

type DpiAwarenessContext = *mut c_void;
type ProcessDpiAwareness = i32;
type SetProcessDpiAwarenessContextFn =
    unsafe extern "system" fn(DpiAwarenessContext) -> windows_sys::core::BOOL;
type SetProcessDpiAwarenessFn =
    unsafe extern "system" fn(ProcessDpiAwareness) -> windows_sys::core::HRESULT;
type SetProcessDpiAwareFn = unsafe extern "system" fn() -> windows_sys::core::BOOL;
type EnableNonClientDpiScalingFn = unsafe extern "system" fn(HWND) -> windows_sys::core::BOOL;
type GetDpiForWindowFn = unsafe extern "system" fn(HWND) -> u32;

macro_rules! load_function {
    ($library:literal, $function:ident, $function_type:ty) => {{
        let procedure = load_procedure(
            concat!($library, "\0").as_bytes(),
            concat!(stringify!($function), "\0").as_bytes(),
        );
        procedure.map(|procedure| {
            // SAFETY: The symbol name and target function pointer type are paired at each call
            // site with the corresponding Win32 API signature.
            unsafe {
                mem::transmute::<unsafe extern "system" fn() -> isize, $function_type>(procedure)
            }
        })
    }};
}

fn enable_process_dpi_awareness() {
    static ENABLE_DPI_AWARENESS: Once = Once::new();

    ENABLE_DPI_AWARENESS.call_once(|| {
        if let Some(set_awareness_context) = load_function!(
            "user32.dll",
            SetProcessDpiAwarenessContext,
            SetProcessDpiAwarenessContextFn
        ) {
            let ok = unsafe { set_awareness_context(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
            if ok == 0 {
                let _ = unsafe { set_awareness_context(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE) };
            }
            return;
        }

        if let Some(set_awareness) = load_function!(
            "shcore.dll",
            SetProcessDpiAwareness,
            SetProcessDpiAwarenessFn
        ) {
            let _ = unsafe { set_awareness(PROCESS_PER_MONITOR_DPI_AWARE) };
            return;
        }

        if let Some(set_dpi_aware) =
            load_function!("user32.dll", SetProcessDPIAware, SetProcessDpiAwareFn)
        {
            let _ = unsafe { set_dpi_aware() };
        }
    });
}

fn enable_non_client_dpi_scaling(hwnd: HWND) {
    static ENABLE_NON_CLIENT_DPI_SCALING: OnceLock<Option<EnableNonClientDpiScalingFn>> =
        OnceLock::new();

    if hwnd.is_null() {
        return;
    }

    // load_procedure keeps the loaded module referenced, so this cached symbol
    // remains valid for the process lifetime without reloading user32.dll.
    let enable_scaling = *ENABLE_NON_CLIENT_DPI_SCALING.get_or_init(|| {
        load_function!(
            "user32.dll",
            EnableNonClientDpiScaling,
            EnableNonClientDpiScalingFn
        )
    });

    if let Some(enable_scaling) = enable_scaling {
        let _ = unsafe { enable_scaling(hwnd) };
    }
}

fn dpi_y_for_window(hwnd: HWND) -> i32 {
    if !hwnd.is_null()
        && let Some(dpi) = dpi_for_window(hwnd)
    {
        return dpi as i32;
    }

    let hdc = unsafe { GetDC(hwnd) };
    if hdc.is_null() {
        return DEFAULT_DPI as i32;
    }

    let dpi_y = unsafe { GetDeviceCaps(hdc, LOGPIXELSY as i32) };
    unsafe {
        ReleaseDC(hwnd, hdc);
    }
    if dpi_y > 0 { dpi_y } else { DEFAULT_DPI as i32 }
}

fn suggested_rect_from_dpi_change(lparam: LPARAM) -> Option<RECT> {
    if lparam == 0 {
        return None;
    }

    unsafe { (lparam as *const RECT).as_ref().copied() }
}

fn dpi_for_window(hwnd: HWND) -> Option<u32> {
    static GET_DPI_FOR_WINDOW: OnceLock<Option<GetDpiForWindowFn>> = OnceLock::new();

    let get_dpi = (*GET_DPI_FOR_WINDOW
        .get_or_init(|| load_function!("user32.dll", GetDpiForWindow, GetDpiForWindowFn)))?;
    let dpi = unsafe { get_dpi(hwnd) };
    (dpi > 0).then_some(dpi)
}

fn load_procedure(library: &'static [u8], function: &'static [u8]) -> FARPROC {
    debug_assert_eq!(library.last(), Some(&0));
    debug_assert_eq!(function.last(), Some(&0));

    let module =
        unsafe { LoadLibraryExA(library.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
    if module.is_null() {
        return None;
    }

    unsafe { GetProcAddress(module, function.as_ptr()) }
}

fn system_prefers_dark_theme() -> bool {
    let subkey = wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let value = wide_null("AppsUseLightTheme");
    let mut data = 1u32;
    let mut data_size = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut data as *mut u32).cast(),
            &mut data_size,
        )
    };
    status == ERROR_SUCCESS && data == 0
}

fn delete_brush(brush: &mut HBRUSH) {
    if !brush.is_null() {
        unsafe {
            DeleteObject(*brush as _);
        }
        *brush = null_mut();
    }
}

const fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    red as COLORREF | ((green as COLORREF) << 8) | ((blue as COLORREF) << 16)
}

fn control_id(id: u16) -> HMENU {
    id as isize as HMENU
}

fn resource_id(id: u16) -> *const u16 {
    id as usize as *const u16
}

fn context_menu_screen_point(lparam: LPARAM, fallback_hwnd: HWND) -> Result<POINT, AppError> {
    if lparam == -1 {
        return window_center_screen_point(fallback_hwnd, "get context menu target rectangle");
    }

    let packed = lparam as u32;
    Ok(POINT {
        x: (packed & 0xffff) as i16 as i32,
        y: ((packed >> 16) & 0xffff) as i16 as i32,
    })
}

fn window_center_screen_point(hwnd: HWND, context: &'static str) -> Result<POINT, AppError> {
    let mut rect = RECT::default();
    let ok = unsafe { GetWindowRect(hwnd, &mut rect) };
    if ok == 0 {
        return Err(last_win32_error(context));
    }

    Ok(POINT {
        x: rect.left + (rect.right - rect.left) / 2,
        y: rect.top + (rect.bottom - rect.top) / 2,
    })
}

fn loword(value: usize) -> u16 {
    (value & 0xffff) as u16
}

fn hiword(value: usize) -> u16 {
    ((value >> 16) & 0xffff) as u16
}

fn menu_text(value: &str) -> String {
    value.replace('&', "&&")
}

fn points_to_logical_height(font_size_pt: u32, dpi_y: i32) -> i32 {
    let dpi_y = dpi_y.max(1);
    -(((i64::from(font_size_pt) * i64::from(dpi_y)) + 36) / 72) as i32
}

fn scale_px_for_dpi(value: i32, dpi_y: i32) -> i32 {
    let dpi_y = i64::from(dpi_y.max(1));
    (((i64::from(value) * dpi_y) + 48) / 96) as i32
}

fn selected_point_size(hwnd: HWND, point_size_tenths: i32, logical_height: i32) -> u32 {
    if point_size_tenths > 0 {
        return (((point_size_tenths as u32) + 5) / 10).clamp(MIN_FONT_SIZE_PT, MAX_FONT_SIZE_PT);
    }

    let dpi_y = i64::from(dpi_y_for_window(hwnd).max(1));
    let point_size = ((i64::from(logical_height).abs() * 72) + (dpi_y / 2)) / dpi_y;
    (point_size as u32).clamp(MIN_FONT_SIZE_PT, MAX_FONT_SIZE_PT)
}

fn set_logfont_face_name(logfont: &mut LOGFONTW, face_name: &str) {
    let max = logfont.lfFaceName.len().saturating_sub(1);
    for (slot, ch) in logfont
        .lfFaceName
        .iter_mut()
        .take(max)
        .zip(face_name.encode_utf16())
    {
        *slot = ch;
    }
}

fn logfont_face_name(logfont: &LOGFONTW) -> Option<String> {
    let length = logfont
        .lfFaceName
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(logfont.lfFaceName.len());
    if length == 0 {
        return None;
    }
    String::from_utf16(&logfont.lfFaceName[..length]).ok()
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
