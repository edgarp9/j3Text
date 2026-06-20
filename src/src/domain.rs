use std::borrow::Cow;
use std::convert::Infallible;
use std::ffi::OsString;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

pub const LARGE_FILE_THRESHOLD_BYTES: u64 = 100 * 1024 * 1024;
pub const MAX_DOCUMENT_LOAD_BYTES: u64 = LARGE_FILE_THRESHOLD_BYTES;
pub const VISIBLE_WHITESPACE_RENDER_LIMIT_BYTES: usize = (LARGE_FILE_THRESHOLD_BYTES as usize) / 4;
pub const MAX_RECENT_FILES: usize = 10;
pub const MAX_SEARCH_RESULTS: usize = 2000;
pub const MIN_FONT_SIZE_PT: u32 = 4;
pub const MAX_FONT_SIZE_PT: u32 = 288;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DocumentId(u64);

impl DocumentId {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    EucKr,
    Cp949,
    ShiftJis,
    Gb18030,
    Big5,
    Iso88591,
    Windows1250,
    Windows1251,
    Windows1252,
    Windows1253,
    Windows1254,
    Windows1255,
    Windows1256,
    Windows1257,
    Windows874,
}

impl TextEncoding {
    pub const ALL: [Self; 19] = [
        Self::Utf8,
        Self::Utf8Bom,
        Self::Utf16Le,
        Self::Utf16Be,
        Self::EucKr,
        Self::Cp949,
        Self::ShiftJis,
        Self::Gb18030,
        Self::Big5,
        Self::Iso88591,
        Self::Windows1250,
        Self::Windows1251,
        Self::Windows1252,
        Self::Windows1253,
        Self::Windows1254,
        Self::Windows1255,
        Self::Windows1256,
        Self::Windows1257,
        Self::Windows874,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 BOM",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::EucKr => "EUC-KR",
            Self::Cp949 => "CP949",
            Self::ShiftJis => "Shift-JIS",
            Self::Gb18030 => "GB18030",
            Self::Big5 => "Big5",
            Self::Iso88591 => "ISO-8859-1",
            Self::Windows1250 => "Windows-1250",
            Self::Windows1251 => "Windows-1251",
            Self::Windows1252 => "Windows-1252",
            Self::Windows1253 => "Windows-1253",
            Self::Windows1254 => "Windows-1254",
            Self::Windows1255 => "Windows-1255",
            Self::Windows1256 => "Windows-1256",
            Self::Windows1257 => "Windows-1257",
            Self::Windows874 => "Windows-874",
        }
    }

    pub fn from_user_input(input: &str) -> Option<Self> {
        let normalized = normalize_encoding_name(input);
        if normalized.is_empty() {
            return None;
        }

        let alias = match normalized.as_str() {
            "utf8sig" | "utf8signature" | "utf8withbom" => Some(Self::Utf8Bom),
            "latin1" | "latin" => Some(Self::Iso88591),
            "sjis" | "mskanji" => Some(Self::ShiftJis),
            "cp1250" => Some(Self::Windows1250),
            "cp1251" => Some(Self::Windows1251),
            "cp1252" => Some(Self::Windows1252),
            "cp1253" => Some(Self::Windows1253),
            "cp1254" => Some(Self::Windows1254),
            "cp1255" => Some(Self::Windows1255),
            "cp1256" => Some(Self::Windows1256),
            "cp1257" => Some(Self::Windows1257),
            "cp874" => Some(Self::Windows874),
            _ => None,
        };

        Self::ALL
            .iter()
            .copied()
            .find(|encoding| normalize_encoding_name(encoding.display_name()) == normalized)
            .or(alias)
    }

    pub fn can_encode_all_unicode(self) -> bool {
        matches!(
            self,
            Self::Utf8 | Self::Utf8Bom | Self::Utf16Le | Self::Utf16Be
        )
    }
}

fn normalize_encoding_name(input: &str) -> String {
    input
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Crlf,
    Lf,
    Cr,
}

impl LineEnding {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Crlf => "CRLF",
            Self::Lf => "LF",
            Self::Cr => "CR",
        }
    }

    pub fn sequence(self) -> &'static str {
        match self {
            Self::Crlf => "\r\n",
            Self::Lf => "\n",
            Self::Cr => "\r",
        }
    }

    pub fn detect(text: &str) -> Self {
        let (crlf, lf, cr) = line_ending_counts(text.as_bytes());
        Self::from_counts(crlf, lf, cr)
    }

    fn from_counts(crlf: usize, lf: usize, cr: usize) -> Self {
        if crlf >= lf && crlf >= cr {
            Self::Crlf
        } else if lf >= cr {
            Self::Lf
        } else {
            Self::Cr
        }
    }

    pub fn normalize_text(self, text: &str) -> String {
        let mut normalized = String::with_capacity(text.len());
        let result: Result<(), Infallible> = self.try_for_each_normalized_char(text, |ch| {
            normalized.push(ch);
            Ok(())
        });
        match result {
            Ok(()) => normalized,
            Err(never) => match never {},
        }
    }

    pub fn is_normalized_text(self, text: &str) -> bool {
        self.normalized_prefix_len(text) == text.len()
    }

    pub(crate) fn normalized_prefix_len(self, text: &str) -> usize {
        let bytes = text.as_bytes();
        match self {
            Self::Lf => bytes
                .iter()
                .position(|byte| *byte == b'\r')
                .unwrap_or(bytes.len()),
            Self::Cr => bytes
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |index| {
                    if index > 0 && bytes[index - 1] == b'\r' {
                        index - 1
                    } else {
                        index
                    }
                }),
            Self::Crlf => {
                let mut index = 0;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\r' => {
                            if bytes.get(index + 1) != Some(&b'\n') {
                                return index;
                            }
                            index += 2;
                        }
                        b'\n' => return index,
                        _ => index += 1,
                    }
                }
                bytes.len()
            }
        }
    }

    pub fn try_for_each_normalized_char<E, F>(self, text: &str, mut emit: F) -> Result<(), E>
    where
        F: FnMut(char) -> Result<(), E>,
    {
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '\r' => {
                    if matches!(chars.peek(), Some('\n')) {
                        chars.next();
                    }
                    for replacement in self.sequence().chars() {
                        emit(replacement)?;
                    }
                }
                '\n' => {
                    for replacement in self.sequence().chars() {
                        emit(replacement)?;
                    }
                }
                _ => emit(ch)?,
            }
        }

        Ok(())
    }
}

fn line_ending_counts(bytes: &[u8]) -> (usize, usize, usize) {
    let mut crlf = 0usize;
    let mut lf = 0usize;
    let mut cr = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\r' if index + 1 < bytes.len() && bytes[index + 1] == b'\n' => {
                crlf += 1;
                index += 2;
            }
            b'\r' => {
                cr += 1;
                index += 1;
            }
            b'\n' => {
                lf += 1;
                index += 1;
            }
            _ => index += 1,
        }
    }

    (crlf, lf, cr)
}

fn is_utf8_continuation_byte(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

fn utf8_char_count(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .filter(|byte| !is_utf8_continuation_byte(**byte))
        .count()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LoadedTextAnalysis {
    pub(crate) line_ending: LineEnding,
    pub(crate) metrics: DocumentMetrics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LoadedTextScan {
    pub(crate) analysis: LoadedTextAnalysis,
    pub(crate) contains_nul: bool,
}

impl LoadedTextAnalysis {
    pub(crate) fn scan_text(text: &str) -> LoadedTextScan {
        let bytes = text.as_bytes();
        let mut crlf = 0usize;
        let mut lf = 0usize;
        let mut cr = 0usize;
        let mut char_count = 0usize;
        let mut contains_nul = false;
        let mut index = 0usize;

        while index < bytes.len() {
            match bytes[index] {
                b'\r' if index + 1 < bytes.len() && bytes[index + 1] == b'\n' => {
                    crlf += 1;
                    char_count += 2;
                    index += 2;
                }
                b'\r' => {
                    cr += 1;
                    char_count += 1;
                    index += 1;
                }
                b'\n' => {
                    lf += 1;
                    char_count += 1;
                    index += 1;
                }
                b'\0' => {
                    contains_nul = true;
                    char_count += 1;
                    index += 1;
                }
                byte if is_utf8_continuation_byte(byte) => {
                    index += 1;
                }
                _ => {
                    char_count += 1;
                    index += 1;
                }
            }
        }

        LoadedTextScan {
            analysis: Self {
                line_ending: LineEnding::from_counts(crlf, lf, cr),
                metrics: DocumentMetrics::from_char_count(char_count),
            },
            contains_nul,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirtyState {
    Clean,
    Dirty,
}

impl DirtyState {
    pub fn is_dirty(self) -> bool {
        matches!(self, Self::Dirty)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    System,
    Light,
    ClassicDark,
    SepiaTeal,
    Graphite,
    Forest,
    SteelBlue,
}

const THEME_MODES: [ThemeMode; 7] = [
    ThemeMode::System,
    ThemeMode::Light,
    ThemeMode::ClassicDark,
    ThemeMode::SepiaTeal,
    ThemeMode::Graphite,
    ThemeMode::Forest,
    ThemeMode::SteelBlue,
];

impl ThemeMode {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::ClassicDark => "Classic Dark",
            Self::SepiaTeal => "Sepia Teal",
            Self::Graphite => "Graphite",
            Self::Forest => "Forest",
            Self::SteelBlue => "Steel Blue",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::ClassicDark => "classic-dark",
            Self::SepiaTeal => "sepia-teal",
            Self::Graphite => "graphite",
            Self::Forest => "forest",
            Self::SteelBlue => "steel-blue",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" | "classic-dark" | "classic_dark" => Some(Self::ClassicDark),
            "sepia-teal" | "sepia_teal" | "sepia" => Some(Self::SepiaTeal),
            "graphite" | "gray" | "grey" => Some(Self::Graphite),
            "forest" | "green" => Some(Self::Forest),
            "steel-blue" | "steel_blue" | "steel" => Some(Self::SteelBlue),
            _ => None,
        }
    }

    pub fn options() -> &'static [Self] {
        &THEME_MODES
    }

    pub fn uses_dark_mode(self) -> bool {
        !matches!(self, Self::System | Self::Light)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutKey {
    Character(char),
    Function(u8),
}

impl ShortcutKey {
    pub fn display_name(self) -> String {
        match self {
            Self::Character(ch) => ch.to_string(),
            Self::Function(number) => format!("F{number}"),
        }
    }

    pub fn storage_key(self) -> String {
        match self {
            Self::Character(ch) => ch.to_ascii_lowercase().to_string(),
            Self::Function(number) => format!("f{number}"),
        }
    }

    fn from_storage_key(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }

        let lower = value.to_ascii_lowercase();
        if let Some(number) = lower.strip_prefix('f')
            && let Ok(number) = number.parse::<u8>()
            && (1..=24).contains(&number)
        {
            return Some(Self::Function(number));
        }

        let mut chars = value.chars();
        let ch = chars.next()?;
        if chars.next().is_none() && ch.is_ascii_alphanumeric() {
            Some(Self::Character(ch.to_ascii_uppercase()))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyboardShortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: ShortcutKey,
}

impl KeyboardShortcut {
    pub const CTRL_N: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Character('N'),
    };

    pub const CTRL_O: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Character('O'),
    };

    pub const CTRL_S: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Character('S'),
    };

    pub const CTRL_SHIFT_S: Self = Self {
        ctrl: true,
        alt: false,
        shift: true,
        key: ShortcutKey::Character('S'),
    };

    pub const CTRL_W: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Character('W'),
    };

    pub const CTRL_SHIFT_W: Self = Self {
        ctrl: true,
        alt: false,
        shift: true,
        key: ShortcutKey::Character('W'),
    };

    pub const CTRL_F4: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Function(4),
    };

    pub const CTRL_F: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Character('F'),
    };

    pub const CTRL_SHIFT_F: Self = Self {
        ctrl: true,
        alt: false,
        shift: true,
        key: ShortcutKey::Character('F'),
    };

    pub const CTRL_H: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Character('H'),
    };

    pub const F3: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        key: ShortcutKey::Function(3),
    };

    pub const SHIFT_F3: Self = Self {
        ctrl: false,
        alt: false,
        shift: true,
        key: ShortcutKey::Function(3),
    };

    pub const CTRL_SHIFT_P: Self = Self {
        ctrl: true,
        alt: false,
        shift: true,
        key: ShortcutKey::Character('P'),
    };

    pub const ALT_Z: Self = Self {
        ctrl: false,
        alt: true,
        shift: false,
        key: ShortcutKey::Character('Z'),
    };

    pub const CTRL_A: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Character('A'),
    };

    pub const CTRL_Z: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Character('Z'),
    };

    pub const CTRL_Y: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Character('Y'),
    };

    pub const CTRL_X: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Character('X'),
    };

    pub const CTRL_C: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Character('C'),
    };

    pub const CTRL_V: Self = Self {
        ctrl: true,
        alt: false,
        shift: false,
        key: ShortcutKey::Character('V'),
    };

    pub fn display_name(self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        parts.push(self.key.display_name());
        parts.join("+")
    }

    pub fn storage_key(self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("ctrl".to_string());
        }
        if self.alt {
            parts.push("alt".to_string());
        }
        if self.shift {
            parts.push("shift".to_string());
        }
        parts.push(self.key.storage_key());
        parts.join("+")
    }

    pub fn from_storage_key(value: &str) -> Option<Self> {
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut key = None;

        for part in value
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" if !ctrl => ctrl = true,
                "alt" if !alt => alt = true,
                "shift" if !shift => shift = true,
                _ if key.is_none() => key = ShortcutKey::from_storage_key(part),
                _ => return None,
            }
        }

        let shortcut = Self {
            ctrl,
            alt,
            shift,
            key: key?,
        };
        shortcut.is_safe_for_text_editor().then_some(shortcut)
    }

    pub fn is_safe_for_text_editor(self) -> bool {
        self.ctrl || self.alt || matches!(self.key, ShortcutKey::Function(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorShortcuts {
    pub new_file: Option<KeyboardShortcut>,
    pub open_file: Option<KeyboardShortcut>,
    pub save: Option<KeyboardShortcut>,
    pub save_as: Option<KeyboardShortcut>,
    pub close_tab: Option<KeyboardShortcut>,
    pub find: Option<KeyboardShortcut>,
    pub replace: Option<KeyboardShortcut>,
    pub find_all: Option<KeyboardShortcut>,
    pub find_next: Option<KeyboardShortcut>,
    pub find_previous: Option<KeyboardShortcut>,
    pub command_palette: Option<KeyboardShortcut>,
    pub toggle_word_wrap: Option<KeyboardShortcut>,
    pub select_all: Option<KeyboardShortcut>,
    pub undo: Option<KeyboardShortcut>,
    pub redo: Option<KeyboardShortcut>,
    pub cut: Option<KeyboardShortcut>,
    pub copy: Option<KeyboardShortcut>,
    pub paste: Option<KeyboardShortcut>,
}

impl Default for EditorShortcuts {
    fn default() -> Self {
        Self {
            new_file: Some(KeyboardShortcut::CTRL_N),
            open_file: Some(KeyboardShortcut::CTRL_O),
            save: Some(KeyboardShortcut::CTRL_S),
            save_as: Some(KeyboardShortcut::CTRL_SHIFT_S),
            close_tab: Some(KeyboardShortcut::CTRL_W),
            find: Some(KeyboardShortcut::CTRL_F),
            replace: Some(KeyboardShortcut::CTRL_H),
            find_all: Some(KeyboardShortcut::CTRL_SHIFT_F),
            find_next: Some(KeyboardShortcut::F3),
            find_previous: Some(KeyboardShortcut::SHIFT_F3),
            command_palette: Some(KeyboardShortcut::CTRL_SHIFT_P),
            toggle_word_wrap: Some(KeyboardShortcut::ALT_Z),
            select_all: Some(KeyboardShortcut::CTRL_A),
            undo: Some(KeyboardShortcut::CTRL_Z),
            redo: Some(KeyboardShortcut::CTRL_Y),
            cut: Some(KeyboardShortcut::CTRL_X),
            copy: Some(KeyboardShortcut::CTRL_C),
            paste: Some(KeyboardShortcut::CTRL_V),
        }
    }
}

impl EditorShortcuts {
    pub fn sanitized(mut self) -> Self {
        for command in EditorCommandId::SHORTCUT_COMMANDS {
            if self
                .shortcut_for(command)
                .is_some_and(|shortcut| !shortcut.is_safe_for_text_editor())
            {
                self.set_shortcut(command, command.default_shortcut());
            }
        }

        let mut used = Vec::new();
        for command in EditorCommandId::SHORTCUT_COMMANDS {
            let Some(shortcut) = self.shortcut_for(command) else {
                continue;
            };
            if used.contains(&shortcut) {
                self.set_shortcut(command, None);
            } else {
                used.push(shortcut);
            }
        }
        self
    }

    pub fn shortcut_for(&self, command: EditorCommandId) -> Option<KeyboardShortcut> {
        match command {
            EditorCommandId::NewFile => self.new_file,
            EditorCommandId::OpenFile => self.open_file,
            EditorCommandId::Save => self.save,
            EditorCommandId::SaveAs => self.save_as,
            EditorCommandId::CloseTab => self.close_tab,
            EditorCommandId::Find => self.find,
            EditorCommandId::Replace => self.replace,
            EditorCommandId::FindAll => self.find_all,
            EditorCommandId::FindNext => self.find_next,
            EditorCommandId::FindPrevious => self.find_previous,
            EditorCommandId::CommandPalette => self.command_palette,
            EditorCommandId::ToggleWordWrap => self.toggle_word_wrap,
            EditorCommandId::SelectAll => self.select_all,
            EditorCommandId::Undo => self.undo,
            EditorCommandId::Redo => self.redo,
            EditorCommandId::Cut => self.cut,
            EditorCommandId::Copy => self.copy,
            EditorCommandId::Paste => self.paste,
            EditorCommandId::CloseOtherTabs
            | EditorCommandId::ToggleLineNumbers
            | EditorCommandId::ToggleVisibleWhitespace
            | EditorCommandId::ReopenWithEncoding
            | EditorCommandId::ConvertEncoding
            | EditorCommandId::SetLineEnding(_) => None,
        }
    }

    pub fn set_shortcut(
        &mut self,
        command: EditorCommandId,
        shortcut: Option<KeyboardShortcut>,
    ) -> bool {
        match command {
            EditorCommandId::NewFile => self.new_file = shortcut,
            EditorCommandId::OpenFile => self.open_file = shortcut,
            EditorCommandId::Save => self.save = shortcut,
            EditorCommandId::SaveAs => self.save_as = shortcut,
            EditorCommandId::CloseTab => self.close_tab = shortcut,
            EditorCommandId::Find => self.find = shortcut,
            EditorCommandId::Replace => self.replace = shortcut,
            EditorCommandId::FindAll => self.find_all = shortcut,
            EditorCommandId::FindNext => self.find_next = shortcut,
            EditorCommandId::FindPrevious => self.find_previous = shortcut,
            EditorCommandId::CommandPalette => self.command_palette = shortcut,
            EditorCommandId::ToggleWordWrap => self.toggle_word_wrap = shortcut,
            EditorCommandId::SelectAll => self.select_all = shortcut,
            EditorCommandId::Undo => self.undo = shortcut,
            EditorCommandId::Redo => self.redo = shortcut,
            EditorCommandId::Cut => self.cut = shortcut,
            EditorCommandId::Copy => self.copy = shortcut,
            EditorCommandId::Paste => self.paste = shortcut,
            EditorCommandId::CloseOtherTabs
            | EditorCommandId::ToggleLineNumbers
            | EditorCommandId::ToggleVisibleWhitespace
            | EditorCommandId::ReopenWithEncoding
            | EditorCommandId::ConvertEncoding
            | EditorCommandId::SetLineEnding(_) => return false,
        }
        true
    }

    pub fn command_for(&self, shortcut: KeyboardShortcut) -> Option<EditorCommandId> {
        EditorCommandId::SHORTCUT_COMMANDS
            .into_iter()
            .find(|command| self.shortcut_for(*command) == Some(shortcut))
    }

    pub fn clear_matching_shortcut(
        &mut self,
        shortcut: KeyboardShortcut,
        except: EditorCommandId,
    ) -> Option<EditorCommandId> {
        let command = self
            .command_for(shortcut)
            .filter(|command| *command != except)?;
        self.set_shortcut(command, None);
        Some(command)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorSettings {
    pub font_name: String,
    pub font_size_pt: u32,
    pub tab_size: u8,
    pub word_wrap: bool,
    pub show_whitespace: bool,
    pub theme: ThemeMode,
    pub shortcuts: EditorShortcuts,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            font_name: "Consolas".to_string(),
            font_size_pt: 11,
            tab_size: 4,
            word_wrap: false,
            show_whitespace: false,
            theme: ThemeMode::System,
            shortcuts: EditorShortcuts::default(),
        }
    }
}

impl EditorSettings {
    pub fn sanitized(mut self) -> Self {
        if self.font_name.trim().is_empty() {
            self.font_name = "Consolas".to_string();
        }
        self.font_size_pt = self.font_size_pt.clamp(MIN_FONT_SIZE_PT, MAX_FONT_SIZE_PT);
        self.tab_size = self.tab_size.clamp(1, 16);
        self.shortcuts = self.shortcuts.sanitized();
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileSnapshot {
    pub modified: Option<SystemTime>,
    pub byte_len: u64,
}

impl FileSnapshot {
    pub fn has_changed_from(self, current: Self) -> bool {
        self.byte_len != current.byte_len || self.modified != current.modified
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadOnlyReason {
    LargeFile,
    FileAttribute,
    Policy,
}

impl ReadOnlyReason {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::LargeFile => "Large",
            Self::FileAttribute => "Read-only",
            Self::Policy => "Locked",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::LargeFile => "large_file",
            Self::FileAttribute => "file_attribute",
            Self::Policy => "policy",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value {
            "large_file" => Some(Self::LargeFile),
            "file_attribute" => Some(Self::FileAttribute),
            "policy" => Some(Self::Policy),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LoadedDocument {
    pub path: PathBuf,
    pub content: String,
    pub encoding: TextEncoding,
    pub line_ending: LineEnding,
    pub snapshot: Option<FileSnapshot>,
    pub read_only_reason: Option<ReadOnlyReason>,
}

#[derive(Clone, Debug)]
pub struct Document {
    id: DocumentId,
    title: String,
    path: Option<PathBuf>,
    content: Arc<str>,
    content_generation: u64,
    clean_baseline: Option<CleanBaseline>,
    content_matches_clean_baseline: bool,
    metrics: DocumentMetrics,
    encoding: TextEncoding,
    line_ending: LineEnding,
    dirty_state: DirtyState,
    read_only_reason: Option<ReadOnlyReason>,
    snapshot: Option<FileSnapshot>,
    backing_file_missing: bool,
}

#[derive(Clone, Debug)]
struct CleanBaseline {
    content: Arc<str>,
    metrics: DocumentMetrics,
    encoding: TextEncoding,
    line_ending: LineEnding,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DocumentMetrics {
    char_count: usize,
}

impl DocumentMetrics {
    fn from_text(text: &str) -> Self {
        let bytes = text.as_bytes();
        Self {
            char_count: if bytes.is_ascii() {
                bytes.len()
            } else {
                utf8_char_count(bytes)
            },
        }
    }

    pub(crate) fn from_char_count(char_count: usize) -> Self {
        Self { char_count }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContentUpdateIntent {
    Verify,
    ChangedView,
}

impl ContentUpdateIntent {
    fn current_content_is_known_changed(self) -> bool {
        matches!(self, Self::ChangedView)
    }
}

impl Document {
    pub fn new_untitled(id: DocumentId, sequence: u32) -> Self {
        let content = Arc::from("");
        Self {
            id,
            title: format!("Untitled {sequence}"),
            path: None,
            content: Arc::clone(&content),
            content_generation: 0,
            clean_baseline: Some(CleanBaseline {
                content,
                metrics: DocumentMetrics::default(),
                encoding: TextEncoding::Utf8,
                line_ending: LineEnding::Crlf,
            }),
            content_matches_clean_baseline: true,
            metrics: DocumentMetrics::default(),
            encoding: TextEncoding::Utf8,
            line_ending: LineEnding::Crlf,
            dirty_state: DirtyState::Clean,
            read_only_reason: None,
            snapshot: None,
            backing_file_missing: false,
        }
    }

    pub fn new_for_path(id: DocumentId, path: PathBuf) -> Self {
        let title = title_from_path(&path).unwrap_or_else(|| "Untitled".to_string());
        let content = Arc::from("");
        Self {
            id,
            title,
            path: Some(path),
            content: Arc::clone(&content),
            content_generation: 0,
            clean_baseline: Some(CleanBaseline {
                content,
                metrics: DocumentMetrics::default(),
                encoding: TextEncoding::Utf8,
                line_ending: LineEnding::Crlf,
            }),
            content_matches_clean_baseline: true,
            metrics: DocumentMetrics::default(),
            encoding: TextEncoding::Utf8,
            line_ending: LineEnding::Crlf,
            dirty_state: DirtyState::Clean,
            read_only_reason: None,
            snapshot: None,
            backing_file_missing: true,
        }
    }

    pub fn from_loaded(id: DocumentId, loaded: LoadedDocument) -> Self {
        let metrics = DocumentMetrics::from_text(&loaded.content);
        Self::from_loaded_with_metrics(id, loaded, metrics)
    }

    pub(crate) fn from_loaded_with_metrics(
        id: DocumentId,
        loaded: LoadedDocument,
        metrics: DocumentMetrics,
    ) -> Self {
        let title = title_from_path(&loaded.path).unwrap_or_else(|| "Untitled".to_string());
        let content: Arc<str> = Arc::from(loaded.content.into_boxed_str());
        Self {
            id,
            title,
            path: Some(loaded.path),
            content: Arc::clone(&content),
            content_generation: 0,
            clean_baseline: Some(CleanBaseline {
                content,
                metrics,
                encoding: loaded.encoding,
                line_ending: loaded.line_ending,
            }),
            content_matches_clean_baseline: true,
            metrics,
            encoding: loaded.encoding,
            line_ending: loaded.line_ending,
            dirty_state: DirtyState::Clean,
            read_only_reason: loaded.read_only_reason,
            snapshot: loaded.snapshot,
            backing_file_missing: false,
        }
    }

    pub fn id(&self) -> DocumentId {
        self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn tab_title(&self) -> String {
        let mut title = if self.is_dirty() {
            format!("*{}", self.title)
        } else {
            self.title.clone()
        };
        if let Some(reason) = self.read_only_reason {
            title.push_str(" [");
            title.push_str(reason.display_name());
            title.push(']');
        }
        title
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub(crate) fn content_generation(&self) -> u64 {
        self.content_generation
    }

    pub fn content_snapshot(&self) -> Arc<str> {
        Arc::clone(&self.content)
    }

    pub fn encoding(&self) -> TextEncoding {
        self.encoding
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn dirty_state(&self) -> DirtyState {
        self.dirty_state
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty_state.is_dirty()
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only_reason.is_some()
    }

    pub fn read_only_reason(&self) -> Option<ReadOnlyReason> {
        self.read_only_reason
    }

    pub fn snapshot(&self) -> Option<FileSnapshot> {
        self.snapshot
    }

    pub fn backing_file_missing(&self) -> bool {
        self.backing_file_missing
    }

    pub fn set_content(&mut self, content: String) {
        if self.is_read_only() {
            return;
        }
        self.set_content_inner(
            content,
            None,
            DocumentMetrics::from_text,
            ContentUpdateIntent::Verify,
        );
    }

    pub(crate) fn set_content_with_metrics(&mut self, content: String, metrics: DocumentMetrics) {
        if self.is_read_only() {
            return;
        }
        self.set_content_inner(
            content,
            Some(metrics),
            |_| metrics,
            ContentUpdateIntent::Verify,
        );
    }

    pub(crate) fn set_changed_view_content_with_metrics(
        &mut self,
        content: String,
        metrics: DocumentMetrics,
    ) {
        if self.is_read_only() {
            return;
        }
        self.set_content_inner(
            content,
            Some(metrics),
            |_| metrics,
            ContentUpdateIntent::ChangedView,
        );
    }

    fn set_content_inner<F>(
        &mut self,
        content: String,
        known_metrics: Option<DocumentMetrics>,
        metrics_for_content: F,
        update_intent: ContentUpdateIntent,
    ) where
        F: FnOnce(&str) -> DocumentMetrics,
    {
        let current_matches_clean_baseline = self.current_content_matches_clean_baseline();
        let known_metrics_differs_from_current =
            known_metrics.is_some_and(|metrics| metrics != self.metrics);
        let current_content_must_differ =
            self.content.len() != content.len() || known_metrics_differs_from_current;
        let incoming_matches_clean_baseline =
            self.incoming_content_matches_clean_baseline(&content, known_metrics);
        let content_changed = match (
            current_matches_clean_baseline,
            incoming_matches_clean_baseline,
        ) {
            (true, true) => false,
            (true, false) | (false, true) => true,
            (false, false) => {
                current_content_must_differ
                    || update_intent.current_content_is_known_changed()
                    || self.content.as_ref() != content
            }
        };

        if content_changed {
            let metrics = match (
                incoming_matches_clean_baseline,
                self.clean_baseline.as_ref(),
            ) {
                (true, Some(clean_baseline)) => clean_baseline.metrics,
                _ => metrics_for_content(&content),
            };
            self.replace_content(content, metrics, incoming_matches_clean_baseline);
        } else {
            self.content_matches_clean_baseline = incoming_matches_clean_baseline;
        }
        self.refresh_dirty_state();
    }

    fn replace_content(
        &mut self,
        content: String,
        metrics: DocumentMetrics,
        content_matches_clean_baseline: bool,
    ) {
        self.metrics = metrics;
        self.content = if content_matches_clean_baseline {
            match self.clean_baseline.as_ref() {
                Some(clean_baseline) => Arc::clone(&clean_baseline.content),
                None => Arc::from(content.into_boxed_str()),
            }
        } else {
            Arc::from(content.into_boxed_str())
        };
        self.content_matches_clean_baseline = content_matches_clean_baseline;
        self.content_generation = self.content_generation.wrapping_add(1);
    }

    pub fn mark_dirty_from_view(&mut self) -> bool {
        if self.is_read_only() || self.dirty_state.is_dirty() {
            return false;
        }
        self.dirty_state = DirtyState::Dirty;
        true
    }

    pub fn set_encoding(&mut self, encoding: TextEncoding) {
        if self.is_read_only() {
            return;
        }
        if self.encoding != encoding {
            self.encoding = encoding;
            self.refresh_dirty_state();
        }
    }

    pub fn set_line_ending(&mut self, line_ending: LineEnding) {
        if self.is_read_only() {
            return;
        }
        if self.line_ending != line_ending {
            self.line_ending = line_ending;
            self.refresh_dirty_state();
        }
    }

    pub fn mark_saved(
        &mut self,
        path: PathBuf,
        encoding: TextEncoding,
        line_ending: LineEnding,
        snapshot: Option<FileSnapshot>,
    ) {
        self.title = title_from_path(&path).unwrap_or_else(|| self.title.clone());
        self.path = Some(path);
        self.encoding = encoding;
        self.line_ending = line_ending;
        self.snapshot = snapshot;
        self.read_only_reason = read_only_reason_for_saved_snapshot(snapshot);
        self.backing_file_missing = false;
        self.clean_baseline = Some(CleanBaseline {
            content: Arc::clone(&self.content),
            metrics: self.metrics,
            encoding,
            line_ending,
        });
        self.content_matches_clean_baseline = true;
        self.dirty_state = DirtyState::Clean;
    }

    pub fn replace_from_loaded(&mut self, loaded: LoadedDocument) {
        let metrics = DocumentMetrics::from_text(&loaded.content);
        self.replace_from_loaded_with_metrics(loaded, metrics);
    }

    pub(crate) fn replace_from_loaded_with_metrics(
        &mut self,
        loaded: LoadedDocument,
        metrics: DocumentMetrics,
    ) {
        self.title = title_from_path(&loaded.path).unwrap_or_else(|| self.title.clone());
        self.path = Some(loaded.path);
        self.metrics = metrics;
        self.content = Arc::from(loaded.content.into_boxed_str());
        self.content_generation = self.content_generation.wrapping_add(1);
        self.encoding = loaded.encoding;
        self.line_ending = loaded.line_ending;
        self.snapshot = loaded.snapshot;
        self.read_only_reason = loaded.read_only_reason;
        self.backing_file_missing = false;
        self.clean_baseline = Some(CleanBaseline {
            content: Arc::clone(&self.content),
            metrics,
            encoding: self.encoding,
            line_ending: self.line_ending,
        });
        self.content_matches_clean_baseline = true;
        self.dirty_state = DirtyState::Clean;
    }

    pub fn char_count(&self) -> usize {
        self.metrics.char_count
    }

    fn refresh_dirty_state(&mut self) {
        if self.clean_baseline.is_none() {
            self.content_matches_clean_baseline = false;
        }
        self.dirty_state = self.dirty_state_for_content_match(self.content_matches_clean_baseline);
    }

    fn current_content_matches_clean_baseline(&self) -> bool {
        let Some(clean_baseline) = &self.clean_baseline else {
            return false;
        };
        self.content_matches_clean_baseline || Arc::ptr_eq(&self.content, &clean_baseline.content)
    }

    fn incoming_content_matches_clean_baseline(
        &self,
        content: &str,
        known_metrics: Option<DocumentMetrics>,
    ) -> bool {
        let Some(clean_baseline) = &self.clean_baseline else {
            return false;
        };
        if content.len() != clean_baseline.content.len() {
            return false;
        }
        if known_metrics.is_some_and(|metrics| metrics != clean_baseline.metrics) {
            return false;
        }
        content == clean_baseline.content.as_ref()
    }

    fn dirty_state_for_content_match(&self, content_matches_clean_baseline: bool) -> DirtyState {
        let Some(clean_baseline) = &self.clean_baseline else {
            return DirtyState::Dirty;
        };
        if content_matches_clean_baseline
            && self.encoding == clean_baseline.encoding
            && self.line_ending == clean_baseline.line_ending
            && !self.backing_file_missing
        {
            DirtyState::Clean
        } else {
            DirtyState::Dirty
        }
    }
}

pub fn should_warn_large_file(byte_len: u64) -> bool {
    byte_len >= LARGE_FILE_THRESHOLD_BYTES
}

fn read_only_reason_for_saved_snapshot(snapshot: Option<FileSnapshot>) -> Option<ReadOnlyReason> {
    snapshot
        .is_some_and(|snapshot| should_warn_large_file(snapshot.byte_len))
        .then_some(ReadOnlyReason::LargeFile)
}

pub fn can_load_document_bytes(byte_len: u64) -> bool {
    byte_len <= MAX_DOCUMENT_LOAD_BYTES
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavePolicy {
    AtomicReplace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavePlan {
    pub target_path: PathBuf,
    pub temp_path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavePolicyError {
    MissingFileName,
    EmptyToken,
}

impl SavePolicy {
    pub fn plan(self, target_path: &Path, token: &str) -> Result<SavePlan, SavePolicyError> {
        if token.trim().is_empty() {
            return Err(SavePolicyError::EmptyToken);
        }

        let file_name = target_path
            .file_name()
            .ok_or(SavePolicyError::MissingFileName)?;
        let mut temp_file_name = OsString::from(".");
        temp_file_name.push(file_name);
        temp_file_name.push(format!(".{token}.j3tmp"));
        let temp_path = match target_path.parent() {
            Some(parent) => parent.join(temp_file_name),
            None => PathBuf::from(temp_file_name),
        };

        Ok(SavePlan {
            target_path: target_path.to_path_buf(),
            temp_path,
        })
    }
}

fn title_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResult {
    pub ordinal: usize,
    pub range: Range<usize>,
    pub utf16_range: Range<usize>,
    pub line: usize,
    pub column: usize,
    pub preview: String,
}

pub fn collect_search_results(text: &str, query: &str, limit: usize) -> Vec<SearchResult> {
    if text.is_empty() || query.is_empty() || limit == 0 {
        return Vec::new();
    }

    let mut results = Vec::new();
    let mut cursor = 0usize;
    let mut position = TextPosition::default();
    while cursor <= text.len() && results.len() < limit {
        let Some(relative) = text[cursor..].find(query) else {
            break;
        };
        let begin = cursor + relative;
        let end = begin + query.len();
        position.advance(&text[cursor..begin]);
        let line = position.line;
        let column = position.column;
        let utf16_start = position.utf16_offset;
        position.advance(&text[begin..end]);
        let utf16_end = position.utf16_offset;
        results.push(SearchResult {
            ordinal: results.len() + 1,
            range: begin..end,
            utf16_range: utf16_start..utf16_end,
            line,
            column,
            preview: search_preview(text, begin),
        });
        cursor = end;
    }
    results
}

#[derive(Clone, Copy, Debug)]
struct TextPosition {
    line: usize,
    column: usize,
    utf16_offset: usize,
    previous_was_cr: bool,
}

impl Default for TextPosition {
    fn default() -> Self {
        Self {
            line: 1,
            column: 1,
            utf16_offset: 0,
            previous_was_cr: false,
        }
    }
}

impl TextPosition {
    fn advance(&mut self, text: &str) {
        for ch in text.chars() {
            self.utf16_offset += ch.len_utf16();
            match ch {
                '\r' => {
                    self.line += 1;
                    self.column = 1;
                    self.previous_was_cr = true;
                }
                '\n' if self.previous_was_cr => {
                    self.previous_was_cr = false;
                }
                '\n' => {
                    self.line += 1;
                    self.column = 1;
                    self.previous_was_cr = false;
                }
                _ => {
                    self.column += 1;
                    self.previous_was_cr = false;
                }
            }
        }
    }
}

pub fn find_text(
    text: &str,
    query: &str,
    start_byte: usize,
    direction: SearchDirection,
) -> Option<Range<usize>> {
    if query.is_empty() || text.is_empty() {
        return None;
    }

    let start = floor_char_boundary(text, start_byte.min(text.len()));
    match direction {
        SearchDirection::Forward => find_forward(text, query, start),
        SearchDirection::Backward => find_backward(text, query, start),
    }
}

fn find_forward(text: &str, query: &str, start: usize) -> Option<Range<usize>> {
    if let Some(relative) = text[start..].find(query) {
        let begin = start + relative;
        return Some(begin..begin + query.len());
    }

    if start > 0
        && let Some(begin) = text[..start].find(query)
    {
        return Some(begin..begin + query.len());
    }

    None
}

fn find_backward(text: &str, query: &str, start: usize) -> Option<Range<usize>> {
    if start > 0
        && let Some(begin) = text[..start].rfind(query)
    {
        return Some(begin..begin + query.len());
    }

    if start < text.len()
        && let Some(begin) = text[start..].rfind(query)
    {
        let offset = start + begin;
        return Some(offset..offset + query.len());
    }

    None
}

pub fn utf16_offset_to_byte_index(text: &str, utf16_offset: usize) -> usize {
    let mut current_units = 0usize;

    for (byte_index, ch) in text.char_indices() {
        if current_units >= utf16_offset {
            return byte_index;
        }

        current_units += ch.len_utf16();
        if current_units > utf16_offset {
            return byte_index;
        }
    }

    text.len()
}

pub fn byte_index_to_utf16_offset(text: &str, byte_index: usize) -> usize {
    let index = floor_char_boundary(text, byte_index.min(text.len()));
    text[..index].encode_utf16().count()
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn search_preview(text: &str, byte_index: usize) -> String {
    const PREVIEW_CHARS: usize = 120;
    const PREFIX_CONTEXT_CHARS: usize = 40;

    let index = floor_char_boundary(text, byte_index.min(text.len()));
    let mut line_start = 0usize;
    for (prefix_chars, (position, ch)) in text[..index].char_indices().rev().enumerate() {
        if matches!(ch, '\r' | '\n') {
            line_start = position + ch.len_utf8();
            break;
        }
        if prefix_chars >= PREFIX_CONTEXT_CHARS {
            line_start = position + ch.len_utf8();
            break;
        }
    }

    let mut preview = String::new();
    for (preview_chars, ch) in text[line_start..].chars().enumerate() {
        if matches!(ch, '\r' | '\n') || preview_chars >= PREVIEW_CHARS {
            break;
        }
        preview.push(ch);
    }
    preview
}

const VISIBLE_SPACE: char = '\u{00b7}';
const VISIBLE_TAB: char = '\u{2192}';
const VISIBLE_SPACE_EXTRA_UTF8_BYTES: usize = 1;
const VISIBLE_TAB_EXTRA_UTF8_BYTES: usize = 2;

pub fn render_visible_whitespace(text: &str) -> String {
    let mut rendered = String::with_capacity(visible_whitespace_rendered_len(text));
    push_visible_whitespace(&mut rendered, text);
    rendered
}

pub fn can_render_visible_whitespace_bytes(byte_len: usize) -> bool {
    byte_len <= VISIBLE_WHITESPACE_RENDER_LIMIT_BYTES
}

pub fn render_visible_whitespace_for_display(text: &str) -> Option<Cow<'_, str>> {
    if !can_render_visible_whitespace_bytes(text.len()) {
        return None;
    }

    let Some((first_visible_whitespace, _)) = first_visible_whitespace(text) else {
        return Some(Cow::Borrowed(text));
    };

    let rendered_len = first_visible_whitespace
        + visible_whitespace_rendered_len(&text[first_visible_whitespace..]);
    let mut rendered = String::with_capacity(rendered_len);
    rendered.push_str(&text[..first_visible_whitespace]);
    push_visible_whitespace(&mut rendered, &text[first_visible_whitespace..]);
    Some(Cow::Owned(rendered))
}

fn first_visible_whitespace(text: &str) -> Option<(usize, usize)> {
    for (index, byte) in text.bytes().enumerate() {
        if let Some((_, extra_utf8_bytes)) = visible_whitespace_replacement(byte) {
            return Some((index, extra_utf8_bytes));
        }
    }

    None
}

fn visible_whitespace_rendered_len(text: &str) -> usize {
    text.bytes()
        .fold(text.len(), |rendered_len, byte| match byte {
            b' ' => rendered_len + VISIBLE_SPACE_EXTRA_UTF8_BYTES,
            b'\t' => rendered_len + VISIBLE_TAB_EXTRA_UTF8_BYTES,
            _ => rendered_len,
        })
}

fn visible_whitespace_replacement(byte: u8) -> Option<(char, usize)> {
    match byte {
        b' ' => Some((VISIBLE_SPACE, VISIBLE_SPACE_EXTRA_UTF8_BYTES)),
        b'\t' => Some((VISIBLE_TAB, VISIBLE_TAB_EXTRA_UTF8_BYTES)),
        _ => None,
    }
}

fn push_visible_whitespace(rendered: &mut String, text: &str) {
    let mut copied_until = 0;
    for (index, byte) in text.bytes().enumerate() {
        let Some((replacement, _)) = visible_whitespace_replacement(byte) else {
            continue;
        };
        rendered.push_str(&text[copied_until..index]);
        rendered.push(replacement);
        copied_until = index + 1;
    }
    rendered.push_str(&text[copied_until..]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandGroup {
    File,
    Edit,
    Search,
    View,
    Tabs,
    Document,
}

impl CommandGroup {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Edit => "Edit",
            Self::Search => "Find",
            Self::View => "View",
            Self::Tabs => "Tabs",
            Self::Document => "Text",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorCommandId {
    NewFile,
    OpenFile,
    Save,
    SaveAs,
    CloseTab,
    CloseOtherTabs,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Find,
    Replace,
    FindAll,
    FindNext,
    FindPrevious,
    CommandPalette,
    ToggleLineNumbers,
    ToggleVisibleWhitespace,
    ToggleWordWrap,
    ReopenWithEncoding,
    ConvertEncoding,
    SetLineEnding(LineEnding),
}

impl EditorCommandId {
    pub const SHORTCUT_COMMANDS: [Self; 18] = [
        Self::NewFile,
        Self::OpenFile,
        Self::Save,
        Self::SaveAs,
        Self::CloseTab,
        Self::Find,
        Self::Replace,
        Self::FindAll,
        Self::FindNext,
        Self::FindPrevious,
        Self::CommandPalette,
        Self::ToggleWordWrap,
        Self::SelectAll,
        Self::Undo,
        Self::Redo,
        Self::Cut,
        Self::Copy,
        Self::Paste,
    ];

    pub fn default_shortcut(self) -> Option<KeyboardShortcut> {
        match self {
            Self::NewFile => Some(KeyboardShortcut::CTRL_N),
            Self::OpenFile => Some(KeyboardShortcut::CTRL_O),
            Self::Save => Some(KeyboardShortcut::CTRL_S),
            Self::SaveAs => Some(KeyboardShortcut::CTRL_SHIFT_S),
            Self::CloseTab => Some(KeyboardShortcut::CTRL_W),
            Self::Find => Some(KeyboardShortcut::CTRL_F),
            Self::Replace => Some(KeyboardShortcut::CTRL_H),
            Self::FindAll => Some(KeyboardShortcut::CTRL_SHIFT_F),
            Self::FindNext => Some(KeyboardShortcut::F3),
            Self::FindPrevious => Some(KeyboardShortcut::SHIFT_F3),
            Self::CommandPalette => Some(KeyboardShortcut::CTRL_SHIFT_P),
            Self::ToggleWordWrap => Some(KeyboardShortcut::ALT_Z),
            Self::SelectAll => Some(KeyboardShortcut::CTRL_A),
            Self::Undo => Some(KeyboardShortcut::CTRL_Z),
            Self::Redo => Some(KeyboardShortcut::CTRL_Y),
            Self::Cut => Some(KeyboardShortcut::CTRL_X),
            Self::Copy => Some(KeyboardShortcut::CTRL_C),
            Self::Paste => Some(KeyboardShortcut::CTRL_V),
            Self::CloseOtherTabs
            | Self::ToggleLineNumbers
            | Self::ToggleVisibleWhitespace
            | Self::ReopenWithEncoding
            | Self::ConvertEncoding
            | Self::SetLineEnding(_) => None,
        }
    }

    pub fn shortcut_title(self) -> Option<&'static str> {
        match self {
            Self::NewFile => Some("New"),
            Self::OpenFile => Some("Open"),
            Self::Save => Some("Save"),
            Self::SaveAs => Some("Save As"),
            Self::CloseTab => Some("Close"),
            Self::Find => Some("Find"),
            Self::Replace => Some("Replace"),
            Self::FindAll => Some("Find All"),
            Self::FindNext => Some("Find Next"),
            Self::FindPrevious => Some("Find Previous"),
            Self::CommandPalette => Some("Commands"),
            Self::ToggleWordWrap => Some("Word Wrap"),
            Self::SelectAll => Some("Select All"),
            Self::Undo => Some("Undo"),
            Self::Redo => Some("Redo"),
            Self::Cut => Some("Cut"),
            Self::Copy => Some("Copy"),
            Self::Paste => Some("Paste"),
            Self::CloseOtherTabs
            | Self::ToggleLineNumbers
            | Self::ToggleVisibleWhitespace
            | Self::ReopenWithEncoding
            | Self::ConvertEncoding
            | Self::SetLineEnding(_) => None,
        }
    }

    pub fn shortcut_storage_key(self) -> Option<&'static str> {
        match self {
            Self::NewFile => Some("shortcut_new_file"),
            Self::OpenFile => Some("shortcut_open_file"),
            Self::Save => Some("shortcut_save"),
            Self::SaveAs => Some("shortcut_save_as"),
            Self::CloseTab => Some("shortcut_close_tab"),
            Self::Find => Some("shortcut_find"),
            Self::Replace => Some("shortcut_replace"),
            Self::FindAll => Some("shortcut_find_all"),
            Self::FindNext => Some("shortcut_find_next"),
            Self::FindPrevious => Some("shortcut_find_previous"),
            Self::CommandPalette => Some("shortcut_command_palette"),
            Self::ToggleWordWrap => Some("shortcut_toggle_word_wrap"),
            Self::SelectAll => Some("shortcut_select_all"),
            Self::Undo => Some("shortcut_undo"),
            Self::Redo => Some("shortcut_redo"),
            Self::Cut => Some("shortcut_cut"),
            Self::Copy => Some("shortcut_copy"),
            Self::Paste => Some("shortcut_paste"),
            Self::CloseOtherTabs
            | Self::ToggleLineNumbers
            | Self::ToggleVisibleWhitespace
            | Self::ReopenWithEncoding
            | Self::ConvertEncoding
            | Self::SetLineEnding(_) => None,
        }
    }

    pub fn from_shortcut_storage_key(key: &str) -> Option<Self> {
        Self::SHORTCUT_COMMANDS
            .into_iter()
            .find(|command| command.shortcut_storage_key() == Some(key))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorCommand {
    pub id: EditorCommandId,
    pub group: CommandGroup,
    pub title: &'static str,
}

pub fn all_commands() -> Vec<EditorCommand> {
    let mut commands = vec![
        EditorCommand {
            id: EditorCommandId::NewFile,
            group: CommandGroup::File,
            title: "New",
        },
        EditorCommand {
            id: EditorCommandId::OpenFile,
            group: CommandGroup::File,
            title: "Open",
        },
        EditorCommand {
            id: EditorCommandId::Save,
            group: CommandGroup::File,
            title: "Save",
        },
        EditorCommand {
            id: EditorCommandId::SaveAs,
            group: CommandGroup::File,
            title: "Save As",
        },
        EditorCommand {
            id: EditorCommandId::CloseTab,
            group: CommandGroup::Tabs,
            title: "Close",
        },
        EditorCommand {
            id: EditorCommandId::CloseOtherTabs,
            group: CommandGroup::Tabs,
            title: "Close Others",
        },
        EditorCommand {
            id: EditorCommandId::Undo,
            group: CommandGroup::Edit,
            title: "Undo",
        },
        EditorCommand {
            id: EditorCommandId::Redo,
            group: CommandGroup::Edit,
            title: "Redo",
        },
        EditorCommand {
            id: EditorCommandId::Cut,
            group: CommandGroup::Edit,
            title: "Cut",
        },
        EditorCommand {
            id: EditorCommandId::Copy,
            group: CommandGroup::Edit,
            title: "Copy",
        },
        EditorCommand {
            id: EditorCommandId::Paste,
            group: CommandGroup::Edit,
            title: "Paste",
        },
        EditorCommand {
            id: EditorCommandId::SelectAll,
            group: CommandGroup::Edit,
            title: "Select All",
        },
        EditorCommand {
            id: EditorCommandId::Find,
            group: CommandGroup::Search,
            title: "Find",
        },
        EditorCommand {
            id: EditorCommandId::Replace,
            group: CommandGroup::Search,
            title: "Replace",
        },
        EditorCommand {
            id: EditorCommandId::FindAll,
            group: CommandGroup::Search,
            title: "Find All",
        },
        EditorCommand {
            id: EditorCommandId::FindNext,
            group: CommandGroup::Search,
            title: "Find Next",
        },
        EditorCommand {
            id: EditorCommandId::FindPrevious,
            group: CommandGroup::Search,
            title: "Find Previous",
        },
        EditorCommand {
            id: EditorCommandId::CommandPalette,
            group: CommandGroup::View,
            title: "Commands",
        },
        EditorCommand {
            id: EditorCommandId::ToggleLineNumbers,
            group: CommandGroup::View,
            title: "Line Numbers",
        },
        EditorCommand {
            id: EditorCommandId::ToggleVisibleWhitespace,
            group: CommandGroup::View,
            title: "Marks",
        },
        EditorCommand {
            id: EditorCommandId::ToggleWordWrap,
            group: CommandGroup::View,
            title: "Word Wrap",
        },
    ];

    commands.push(EditorCommand {
        id: EditorCommandId::ReopenWithEncoding,
        group: CommandGroup::Document,
        title: "Reopen Encoding",
    });
    commands.push(EditorCommand {
        id: EditorCommandId::ConvertEncoding,
        group: CommandGroup::Document,
        title: "Change Encoding",
    });

    for (line_ending, title) in [
        (LineEnding::Crlf, "CRLF"),
        (LineEnding::Lf, "LF"),
        (LineEnding::Cr, "CR"),
    ] {
        commands.push(EditorCommand {
            id: EditorCommandId::SetLineEnding(line_ending),
            group: CommandGroup::Document,
            title,
        });
    }

    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_predominant_line_ending() {
        assert_eq!(LineEnding::detect("a\r\nb\r\n"), LineEnding::Crlf);
        assert_eq!(LineEnding::detect("a\nb\n"), LineEnding::Lf);
        assert_eq!(LineEnding::detect("a\rb\r"), LineEnding::Cr);
    }

    #[test]
    fn loaded_text_analysis_counts_chars_while_detecting_line_endings() {
        let text = "a한\r\n😀\r\n끝\n";
        let scan = LoadedTextAnalysis::scan_text(text);
        let analysis = scan.analysis;

        assert!(!scan.contains_nul);
        assert_eq!(analysis.line_ending, LineEnding::Crlf);
        assert_eq!(analysis.metrics, DocumentMetrics::from_text(text));
        assert_eq!(analysis.metrics.char_count, 9);
    }

    #[test]
    fn loaded_text_scan_detects_nul_without_changing_metrics() {
        let text = "a\0한\r\n";
        let scan = LoadedTextAnalysis::scan_text(text);

        assert!(scan.contains_nul);
        assert_eq!(scan.analysis.line_ending, LineEnding::Crlf);
        assert_eq!(scan.analysis.metrics, DocumentMetrics::from_text(text));
        assert_eq!(scan.analysis.metrics.char_count, 5);
    }

    #[test]
    fn normalizes_mixed_line_endings() {
        assert_eq!(LineEnding::Lf.normalize_text("a\r\nb\rc\n"), "a\nb\nc\n");
    }

    #[test]
    fn normalized_prefix_len_starts_at_safe_line_ending_boundary() {
        assert_eq!(LineEnding::Lf.normalized_prefix_len("abc\r\n"), 3);
        assert_eq!(LineEnding::Cr.normalized_prefix_len("abc\r\n"), 3);
        assert_eq!(
            LineEnding::Cr.normalized_prefix_len("abc\rdef"),
            "abc\rdef".len()
        );
        assert_eq!(
            LineEnding::Crlf.normalized_prefix_len("a\r\nb\nc"),
            "a\r\nb".len()
        );
        assert_eq!(
            LineEnding::Crlf.normalized_prefix_len("a\r\nb\rc"),
            "a\r\nb".len()
        );
    }

    #[test]
    fn streamed_line_ending_normalization_uses_same_policy() {
        let mut streamed = String::new();

        LineEnding::Crlf
            .try_for_each_normalized_char("a\r\nb\rc\n", |ch| {
                streamed.push(ch);
                Ok::<_, Infallible>(())
            })
            .unwrap_or_else(|never| match never {});

        assert_eq!(streamed, LineEnding::Crlf.normalize_text("a\r\nb\rc\n"));
        assert_eq!(streamed, "a\r\nb\r\nc\r\n");
    }

    #[test]
    fn finds_forward_and_wraps() {
        assert_eq!(
            find_text("abc abc", "abc", 1, SearchDirection::Forward),
            Some(4..7)
        );
        assert_eq!(
            find_text("abc abc", "abc", 7, SearchDirection::Forward),
            Some(0..3)
        );
    }

    #[test]
    fn maps_utf16_offsets_without_splitting_surrogates() {
        let text = "a😀b";
        assert_eq!(utf16_offset_to_byte_index(text, 2), 1);
        assert_eq!(byte_index_to_utf16_offset(text, text.len()), 4);
    }

    #[test]
    fn large_file_policy_starts_at_100mb() {
        assert!(!should_warn_large_file(LARGE_FILE_THRESHOLD_BYTES - 1));
        assert!(should_warn_large_file(LARGE_FILE_THRESHOLD_BYTES));
        assert!(can_load_document_bytes(MAX_DOCUMENT_LOAD_BYTES));
        assert!(!can_load_document_bytes(MAX_DOCUMENT_LOAD_BYTES + 1));
    }

    #[test]
    fn settings_are_sanitized() {
        let settings = EditorSettings {
            font_name: " ".to_string(),
            font_size_pt: 2,
            tab_size: 32,
            word_wrap: true,
            show_whitespace: true,
            theme: ThemeMode::ClassicDark,
            shortcuts: EditorShortcuts {
                close_tab: Some(KeyboardShortcut {
                    ctrl: false,
                    alt: false,
                    shift: false,
                    key: ShortcutKey::Character('W'),
                }),
                ..EditorShortcuts::default()
            },
        }
        .sanitized();
        assert_eq!(settings.font_name, "Consolas");
        assert_eq!(settings.font_size_pt, MIN_FONT_SIZE_PT);
        assert_eq!(settings.tab_size, 16);
        assert!(settings.word_wrap);
        assert!(settings.show_whitespace);
        assert_eq!(settings.theme, ThemeMode::ClassicDark);
        assert_eq!(settings.shortcuts.close_tab, Some(KeyboardShortcut::CTRL_W));

        let settings = EditorSettings {
            font_size_pt: 500,
            ..EditorSettings::default()
        }
        .sanitized();
        assert_eq!(settings.font_size_pt, MAX_FONT_SIZE_PT);
    }

    #[test]
    fn theme_modes_parse_aliases_and_round_trip_storage_keys() {
        for theme in ThemeMode::options() {
            assert_eq!(ThemeMode::from_key(theme.key()), Some(*theme));
        }

        assert_eq!(ThemeMode::from_key("dark"), Some(ThemeMode::ClassicDark));
        assert_eq!(
            ThemeMode::from_key("classic_dark"),
            Some(ThemeMode::ClassicDark)
        );
        assert_eq!(ThemeMode::from_key("sepia"), Some(ThemeMode::SepiaTeal));
        assert_eq!(ThemeMode::from_key("grey"), Some(ThemeMode::Graphite));
        assert_eq!(ThemeMode::from_key("green"), Some(ThemeMode::Forest));
        assert_eq!(ThemeMode::from_key("steel"), Some(ThemeMode::SteelBlue));
        assert_eq!(ThemeMode::from_key("unknown"), None);
    }

    #[test]
    fn keyboard_shortcuts_parse_display_and_store() {
        let shortcut = KeyboardShortcut::from_storage_key("ctrl+shift+w").expect("parse shortcut");

        assert_eq!(shortcut, KeyboardShortcut::CTRL_SHIFT_W);
        assert_eq!(shortcut.display_name(), "Ctrl+Shift+W");
        assert_eq!(shortcut.storage_key(), "ctrl+shift+w");
        assert_eq!(
            KeyboardShortcut::from_storage_key("Ctrl+F4"),
            Some(KeyboardShortcut::CTRL_F4)
        );
        assert_eq!(KeyboardShortcut::from_storage_key("w"), None);
    }

    #[test]
    fn text_encoding_parses_manual_user_input() {
        assert_eq!(
            TextEncoding::from_user_input(" utf_8 "),
            Some(TextEncoding::Utf8)
        );
        assert_eq!(
            TextEncoding::from_user_input("utf-8-bom"),
            Some(TextEncoding::Utf8Bom)
        );
        assert_eq!(
            TextEncoding::from_user_input("cp1252"),
            Some(TextEncoding::Windows1252)
        );
        assert_eq!(
            TextEncoding::from_user_input("Shift JIS"),
            Some(TextEncoding::ShiftJis)
        );
        assert_eq!(TextEncoding::from_user_input("unknown-codepage"), None);
    }

    #[test]
    fn default_shortcuts_cover_common_editor_commands() {
        let shortcuts = EditorShortcuts::default();

        assert_eq!(
            shortcuts.command_for(KeyboardShortcut::CTRL_S),
            Some(EditorCommandId::Save)
        );
        assert_eq!(
            shortcuts.command_for(KeyboardShortcut::CTRL_SHIFT_S),
            Some(EditorCommandId::SaveAs)
        );
        assert_eq!(
            shortcuts.command_for(KeyboardShortcut::CTRL_F),
            Some(EditorCommandId::Find)
        );
        assert_eq!(
            shortcuts.command_for(KeyboardShortcut::CTRL_A),
            Some(EditorCommandId::SelectAll)
        );
        assert_eq!(
            shortcuts.command_for(KeyboardShortcut::CTRL_Y),
            Some(EditorCommandId::Redo)
        );
        assert_eq!(
            shortcuts.command_for(KeyboardShortcut::F3),
            Some(EditorCommandId::FindNext)
        );
        assert_eq!(
            shortcuts.command_for(KeyboardShortcut::SHIFT_F3),
            Some(EditorCommandId::FindPrevious)
        );
    }

    #[test]
    fn duplicate_shortcuts_are_disabled_during_sanitization() {
        let shortcuts = EditorShortcuts {
            save_as: Some(KeyboardShortcut::CTRL_S),
            ..EditorShortcuts::default()
        }
        .sanitized();

        assert_eq!(shortcuts.save, Some(KeyboardShortcut::CTRL_S));
        assert_eq!(shortcuts.save_as, None);
    }

    #[test]
    fn save_policy_plans_same_directory_temp_file() {
        #[cfg(windows)]
        let path = Path::new("C:\\Temp\\note.txt");
        #[cfg(windows)]
        let expected_temp_path = PathBuf::from("C:\\Temp\\.note.txt.abc.j3tmp");
        #[cfg(not(windows))]
        let path = Path::new("/tmp/note.txt");
        #[cfg(not(windows))]
        let expected_temp_path = PathBuf::from("/tmp/.note.txt.abc.j3tmp");
        let plan = SavePolicy::AtomicReplace
            .plan(path, "abc")
            .expect("save plan");

        assert_eq!(plan.target_path, path);
        assert_eq!(plan.temp_path, expected_temp_path);
    }

    #[test]
    #[cfg(windows)]
    fn save_policy_plans_temp_file_for_non_utf8_windows_file_name() {
        use std::os::windows::ffi::OsStringExt;

        fn path_from_wide(units: &[u16]) -> PathBuf {
            PathBuf::from(OsString::from_wide(units))
        }

        let mut target_units: Vec<u16> = "C:\\Temp\\note".encode_utf16().collect();
        target_units.push(0xD800);
        target_units.extend(".txt".encode_utf16());
        let target_path = path_from_wide(&target_units);

        let plan = SavePolicy::AtomicReplace
            .plan(&target_path, "abc")
            .expect("save plan");

        let mut temp_units: Vec<u16> = "C:\\Temp\\.note".encode_utf16().collect();
        temp_units.push(0xD800);
        temp_units.extend(".txt.abc.j3tmp".encode_utf16());

        assert_eq!(plan.target_path, target_path);
        assert_eq!(plan.temp_path, path_from_wide(&temp_units));
    }

    #[test]
    fn new_document_for_path_keeps_target_without_marking_dirty() {
        let path = PathBuf::from("new-note.txt");
        let document = Document::new_for_path(DocumentId::new(9), path.clone());

        assert_eq!(document.title(), "new-note.txt");
        assert_eq!(document.path(), Some(&path));
        assert_eq!(document.snapshot(), None);
        assert!(document.backing_file_missing());
        assert!(!document.is_dirty());
        assert_eq!(document.encoding(), TextEncoding::Utf8);
        assert_eq!(document.line_ending(), LineEnding::Crlf);
    }

    #[test]
    fn saved_document_clears_missing_backing_file_state() {
        let mut document =
            Document::new_for_path(DocumentId::new(7), PathBuf::from("C:\\Temp\\note.txt"));

        document.mark_saved(
            PathBuf::from("C:\\Temp\\note.txt"),
            TextEncoding::Utf8,
            LineEnding::Crlf,
            Some(FileSnapshot {
                modified: None,
                byte_len: 10,
            }),
        );

        assert!(!document.backing_file_missing());
        assert!(!document.is_dirty());
    }

    #[test]
    fn search_results_include_location_and_preview() {
        let results = collect_search_results("alpha\nbeta alpha\n", "alpha", 10);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].line, 1);
        assert_eq!(results[0].column, 1);
        assert_eq!(results[1].line, 2);
        assert_eq!(results[1].column, 6);
        assert_eq!(results[1].utf16_range, 11..16);
        assert_eq!(results[1].preview, "beta alpha");
    }

    #[test]
    fn search_results_handle_mixed_newlines_and_non_ascii_columns() {
        let text = "한글\r\nxx NEEDLE\rzz NEEDLE\n끝 NEEDLE";
        let results = collect_search_results(text, "NEEDLE", 10);

        assert_eq!(results.len(), 3);
        assert_eq!((results[0].line, results[0].column), (2, 4));
        assert_eq!((results[1].line, results[1].column), (3, 4));
        assert_eq!((results[2].line, results[2].column), (4, 3));
        assert_eq!(results[0].utf16_range, 7..13);
        assert_eq!(results[2].utf16_range, 26..32);
    }

    #[test]
    fn search_results_stay_capped_for_spaced_large_matches() {
        let mut text = String::new();
        for _ in 0..MAX_SEARCH_RESULTS + 50 {
            text.push('가');
            text.extend(std::iter::repeat_n('x', 1024));
            text.push_str("NEEDLE\r\n");
        }

        let results = collect_search_results(&text, "NEEDLE", MAX_SEARCH_RESULTS);

        assert_eq!(results.len(), MAX_SEARCH_RESULTS);
        assert_eq!(results[0].ordinal, 1);
        assert_eq!(results[MAX_SEARCH_RESULTS - 1].ordinal, MAX_SEARCH_RESULTS);
        assert_eq!(results[MAX_SEARCH_RESULTS - 1].line, MAX_SEARCH_RESULTS);
        assert_eq!(results[MAX_SEARCH_RESULTS - 1].column, 1026);
    }

    #[test]
    fn search_preview_is_bounded_on_long_lines() {
        let text = format!("{}NEEDLE{}", "a".repeat(10_000), "b".repeat(10_000));
        let results = collect_search_results(&text, "NEEDLE", 10);

        assert_eq!(results.len(), 1);
        assert!(results[0].preview.contains("NEEDLE"));
        assert!(results[0].preview.chars().count() <= 120);
    }

    #[test]
    fn document_char_count_tracks_non_ascii_edits() {
        let mut document = Document::new_untitled(DocumentId::new(1), 1);

        document.set_content("a한😀".to_string());
        assert_eq!(document.char_count(), 3);

        document.set_content("짧게".to_string());
        assert_eq!(document.char_count(), 2);
    }

    #[test]
    fn document_can_mark_pending_view_edits_dirty_without_copying_text() {
        let mut document = Document::new_untitled(DocumentId::new(1), 1);
        document.set_content("persisted".to_string());
        document.mark_saved(
            PathBuf::from("C:\\Temp\\note.txt"),
            TextEncoding::Utf8,
            LineEnding::Lf,
            None,
        );
        let before = document.content_snapshot();

        document.mark_dirty_from_view();

        assert!(document.is_dirty());
        assert!(std::sync::Arc::ptr_eq(
            &before,
            &document.content_snapshot()
        ));
        assert_eq!(document.content(), "persisted");
    }

    #[test]
    fn synced_view_text_can_return_document_to_clean_baseline() {
        let mut document = Document::from_loaded(
            DocumentId::new(7),
            LoadedDocument {
                path: PathBuf::from("C:\\Temp\\note.txt"),
                content: "clean".to_string(),
                encoding: TextEncoding::Utf8,
                line_ending: LineEnding::Crlf,
                snapshot: Some(FileSnapshot {
                    modified: None,
                    byte_len: 5,
                }),
                read_only_reason: None,
            },
        );

        document.mark_dirty_from_view();
        assert!(document.is_dirty());

        document.set_content("clean".to_string());

        assert!(!document.is_dirty());
        assert_eq!(document.content(), "clean");
    }

    #[test]
    fn synced_view_text_to_clean_baseline_updates_generation_only_for_content_change() {
        let mut document = Document::from_loaded(
            DocumentId::new(7),
            LoadedDocument {
                path: PathBuf::from("C:\\Temp\\note.txt"),
                content: "clean".to_string(),
                encoding: TextEncoding::Utf8,
                line_ending: LineEnding::Crlf,
                snapshot: Some(FileSnapshot {
                    modified: None,
                    byte_len: 5,
                }),
                read_only_reason: None,
            },
        );

        document.set_content_with_metrics("dirty".to_string(), DocumentMetrics::from_text("dirty"));
        let dirty_generation = document.content_generation;

        document.set_content_with_metrics("clean".to_string(), DocumentMetrics::from_text("clean"));

        assert!(!document.is_dirty());
        assert!(document.content_matches_clean_baseline);
        assert!(document.content_generation > dirty_generation);
        let clean_generation = document.content_generation;

        document.mark_dirty_from_view();
        document.set_content_with_metrics("clean".to_string(), DocumentMetrics::from_text("clean"));

        assert!(!document.is_dirty());
        assert_eq!(document.content_generation, clean_generation);
    }

    #[test]
    fn changed_view_sync_trusts_view_change_intent_for_same_length_dirty_content() {
        let mut document = Document::from_loaded(
            DocumentId::new(7),
            LoadedDocument {
                path: PathBuf::from("C:\\Temp\\note.txt"),
                content: "clean".to_string(),
                encoding: TextEncoding::Utf8,
                line_ending: LineEnding::Crlf,
                snapshot: Some(FileSnapshot {
                    modified: None,
                    byte_len: 5,
                }),
                read_only_reason: None,
            },
        );
        document.set_content_with_metrics("draft".to_string(), DocumentMetrics::from_text("draft"));
        let dirty_generation = document.content_generation;
        let dirty_content = document.content_snapshot();

        document.set_changed_view_content_with_metrics(
            "draft".to_string(),
            DocumentMetrics::from_text("draft"),
        );

        assert!(document.is_dirty());
        assert_eq!(document.content(), "draft");
        assert!(document.content_generation > dirty_generation);
        assert!(!std::sync::Arc::ptr_eq(
            &dirty_content,
            &document.content_snapshot()
        ));
    }

    #[test]
    fn changed_view_sync_can_return_same_length_dirty_content_to_clean_baseline() {
        let mut document = Document::from_loaded(
            DocumentId::new(7),
            LoadedDocument {
                path: PathBuf::from("C:\\Temp\\note.txt"),
                content: "clean".to_string(),
                encoding: TextEncoding::Utf8,
                line_ending: LineEnding::Crlf,
                snapshot: Some(FileSnapshot {
                    modified: None,
                    byte_len: 5,
                }),
                read_only_reason: None,
            },
        );
        let clean_content = document.content_snapshot();
        document.set_content_with_metrics("draft".to_string(), DocumentMetrics::from_text("draft"));

        document.set_changed_view_content_with_metrics(
            "clean".to_string(),
            DocumentMetrics::from_text("clean"),
        );

        assert!(!document.is_dirty());
        assert!(document.content_matches_clean_baseline);
        assert!(std::sync::Arc::ptr_eq(
            &clean_content,
            &document.content_snapshot()
        ));
    }

    #[test]
    fn synced_view_text_to_clean_baseline_reuses_baseline_content_and_metrics() {
        let mut document = Document::from_loaded(
            DocumentId::new(7),
            LoadedDocument {
                path: PathBuf::from("C:\\Temp\\note.txt"),
                content: "clean".to_string(),
                encoding: TextEncoding::Utf8,
                line_ending: LineEnding::Crlf,
                snapshot: Some(FileSnapshot {
                    modified: None,
                    byte_len: 5,
                }),
                read_only_reason: None,
            },
        );
        let clean_content = document.content_snapshot();
        document.set_content_with_metrics("dirty".to_string(), DocumentMetrics::from_text("dirty"));

        let mut rescanned = false;
        document.set_content_inner(
            "clean".to_string(),
            None,
            |_| {
                rescanned = true;
                DocumentMetrics::from_text("clean")
            },
            ContentUpdateIntent::Verify,
        );

        assert!(!rescanned);
        assert!(!document.is_dirty());
        assert!(document.content_matches_clean_baseline);
        assert!(std::sync::Arc::ptr_eq(
            &clean_content,
            &document.content_snapshot()
        ));
    }

    #[test]
    fn encoding_and_line_ending_return_to_clean_when_matching_baseline() {
        let mut document = Document::from_loaded(
            DocumentId::new(7),
            LoadedDocument {
                path: PathBuf::from("C:\\Temp\\note.txt"),
                content: "line\r\n".to_string(),
                encoding: TextEncoding::Utf8,
                line_ending: LineEnding::Crlf,
                snapshot: Some(FileSnapshot {
                    modified: None,
                    byte_len: 6,
                }),
                read_only_reason: None,
            },
        );

        document.set_line_ending(LineEnding::Lf);
        assert!(document.is_dirty());
        document.set_line_ending(LineEnding::Crlf);
        assert!(!document.is_dirty());

        document.set_encoding(TextEncoding::Utf8Bom);
        assert!(document.is_dirty());
        document.set_encoding(TextEncoding::Utf8);
        assert!(!document.is_dirty());
    }

    #[test]
    fn read_only_document_rejects_encoding_and_line_ending_mutation() {
        let mut document = Document::from_loaded(
            DocumentId::new(7),
            LoadedDocument {
                path: PathBuf::from("C:\\Temp\\note.txt"),
                content: "locked".to_string(),
                encoding: TextEncoding::Utf8,
                line_ending: LineEnding::Crlf,
                snapshot: Some(FileSnapshot {
                    modified: None,
                    byte_len: 6,
                }),
                read_only_reason: Some(ReadOnlyReason::FileAttribute),
            },
        );

        document.set_encoding(TextEncoding::Utf8Bom);
        document.set_line_ending(LineEnding::Lf);

        assert_eq!(document.encoding(), TextEncoding::Utf8);
        assert_eq!(document.line_ending(), LineEnding::Crlf);
        assert!(!document.is_dirty());
    }

    #[test]
    fn visible_whitespace_keeps_character_positions() {
        assert_eq!(render_visible_whitespace("a b\tc"), "a\u{00b7}b\u{2192}c");
        assert_eq!(
            render_visible_whitespace("a b\tc").encode_utf16().count(),
            "a b\tc".encode_utf16().count()
        );
    }

    #[test]
    fn visible_whitespace_rendered_len_counts_utf8_replacements() {
        let text = "é 한 \t";
        let rendered = render_visible_whitespace(text);

        assert_eq!(rendered, "é\u{00b7}한\u{00b7}\u{2192}");
        assert_eq!(visible_whitespace_rendered_len(text), rendered.len());
        assert!(visible_whitespace_rendered_len(text) > text.len());
    }

    #[test]
    fn visible_whitespace_display_uses_bounded_rendering() {
        assert!(VISIBLE_WHITESPACE_RENDER_LIMIT_BYTES < MAX_DOCUMENT_LOAD_BYTES as usize);
        assert!(can_render_visible_whitespace_bytes(
            VISIBLE_WHITESPACE_RENDER_LIMIT_BYTES
        ));
        assert!(!can_render_visible_whitespace_bytes(
            VISIBLE_WHITESPACE_RENDER_LIMIT_BYTES + 1
        ));
        let oversized = "x".repeat(VISIBLE_WHITESPACE_RENDER_LIMIT_BYTES + 1);
        assert_eq!(render_visible_whitespace_for_display(&oversized), None);

        assert!(matches!(
            render_visible_whitespace_for_display("abc"),
            Some(Cow::Borrowed("abc"))
        ));
        assert_eq!(
            render_visible_whitespace_for_display("a b\tc").as_deref(),
            Some("a\u{00b7}b\u{2192}c")
        );
    }

    #[test]
    fn visible_whitespace_display_reserves_rendered_utf8_len() {
        let text = " \t".repeat(4096);
        let expected = "\u{00b7}\u{2192}".repeat(4096);

        let rendered = match render_visible_whitespace_for_display(&text) {
            Some(Cow::Owned(rendered)) => rendered,
            other => panic!("expected owned rendered whitespace, got {other:?}"),
        };

        assert_eq!(rendered, expected);
        assert_eq!(rendered.capacity(), expected.len());
    }

    #[test]
    fn visible_whitespace_display_preserves_crlf_and_unicode() {
        let text = "한 글\r\nemoji 😀\tend";

        assert_eq!(
            render_visible_whitespace_for_display(text).as_deref(),
            Some("한\u{00b7}글\r\nemoji\u{00b7}😀\u{2192}end")
        );
    }

    #[test]
    fn command_list_contains_core_command_groups() {
        let commands = all_commands();

        assert!(
            commands
                .iter()
                .any(|command| command.id == EditorCommandId::Save)
        );
        assert!(
            commands
                .iter()
                .any(|command| command.id == EditorCommandId::FindAll)
        );
        assert!(
            commands
                .iter()
                .any(|command| command.id == EditorCommandId::CloseOtherTabs)
        );
        assert!(
            commands
                .iter()
                .any(|command| command.id == EditorCommandId::ReopenWithEncoding)
        );
        assert!(
            commands
                .iter()
                .any(|command| command.id == EditorCommandId::ConvertEncoding)
        );
        assert!(
            commands
                .iter()
                .any(|command| { command.id == EditorCommandId::SetLineEnding(LineEnding::Lf) })
        );
    }
}
