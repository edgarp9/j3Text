use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAccessKind {
    PermissionDenied,
    FileInUse,
    ReadOnly,
    NotFound,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodingErrorKind {
    Decode,
    Encode,
    UnsafeText,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformErrorKind {
    RichEditLibraryUnavailable,
    RichEditControlCreateFailed,
    RichEditPlainTextModeFailed,
    MainWindowCreateFailed,
    Other,
}

#[derive(Debug)]
pub enum AppError {
    Io {
        source: io::Error,
        context: &'static str,
        path: Option<PathBuf>,
        user_context: Option<&'static str>,
        user_path: Option<PathBuf>,
        kind: FileAccessKind,
    },
    Win32 {
        code: u32,
        context: &'static str,
        kind: PlatformErrorKind,
    },
    Platform {
        kind: PlatformErrorKind,
        context: &'static str,
        details: String,
    },
    Dialog {
        code: u32,
        context: &'static str,
    },
    Encoding {
        kind: EncodingErrorKind,
        details: String,
    },
    ExternalFileChanged {
        path: PathBuf,
    },
    FileTooLarge {
        path: PathBuf,
        byte_len: u64,
        limit: u64,
    },
    InvalidState(&'static str),
}

impl AppError {
    pub fn io(source: io::Error, context: &'static str) -> Self {
        let kind = classify_io_error(&source);
        Self::Io {
            source,
            context,
            path: None,
            user_context: None,
            user_path: None,
            kind,
        }
    }

    pub fn io_path(source: io::Error, context: &'static str, path: PathBuf) -> Self {
        let kind = classify_io_error(&source);
        Self::Io {
            source,
            context,
            path: Some(path),
            user_context: None,
            user_path: None,
            kind,
        }
    }

    pub fn io_path_with_user(
        source: io::Error,
        context: &'static str,
        path: PathBuf,
        user_context: &'static str,
        user_path: PathBuf,
    ) -> Self {
        let kind = classify_io_error(&source);
        Self::Io {
            source,
            context,
            path: Some(path),
            user_context: Some(user_context),
            user_path: Some(user_path),
            kind,
        }
    }

    pub fn file_access(
        kind: FileAccessKind,
        source: io::Error,
        context: &'static str,
        path: Option<PathBuf>,
    ) -> Self {
        Self::Io {
            source,
            context,
            path,
            user_context: None,
            user_path: None,
            kind,
        }
    }

    pub fn win32(context: &'static str, code: u32) -> Self {
        Self::Win32 {
            code,
            context,
            kind: classify_platform_context(context),
        }
    }

    pub fn platform(
        kind: PlatformErrorKind,
        context: &'static str,
        details: impl Into<String>,
    ) -> Self {
        Self::Platform {
            kind,
            context,
            details: details.into(),
        }
    }

    pub fn dialog(code: u32, context: &'static str) -> Self {
        Self::Dialog { code, context }
    }

    pub fn encoding_decode(details: impl Into<String>) -> Self {
        Self::Encoding {
            kind: EncodingErrorKind::Decode,
            details: details.into(),
        }
    }

    pub fn encoding_encode(details: impl Into<String>) -> Self {
        Self::Encoding {
            kind: EncodingErrorKind::Encode,
            details: details.into(),
        }
    }

    pub fn encoding_unsafe_text(details: impl Into<String>) -> Self {
        Self::Encoding {
            kind: EncodingErrorKind::UnsafeText,
            details: details.into(),
        }
    }

    pub fn external_file_changed(path: PathBuf) -> Self {
        Self::ExternalFileChanged { path }
    }

    pub fn file_too_large(path: PathBuf, byte_len: u64, limit: u64) -> Self {
        Self::FileTooLarge {
            path,
            byte_len,
            limit,
        }
    }

    pub fn with_user_io_context(mut self, user_context: &'static str, user_path: PathBuf) -> Self {
        if let Self::Io {
            user_context: current_context,
            user_path: current_path,
            ..
        } = &mut self
        {
            *current_context = Some(user_context);
            *current_path = Some(user_path);
        }
        self
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::Io {
                context,
                path,
                user_context,
                user_path,
                kind,
                ..
            } => {
                let context = user_context.unwrap_or(context);
                let target = user_path
                    .as_deref()
                    .or(path.as_deref())
                    .map(|path| format!(" ({})", path.display()))
                    .unwrap_or_default();
                match kind {
                    FileAccessKind::PermissionDenied => {
                        format!("No permission to {context}{target}.")
                    }
                    FileAccessKind::FileInUse => {
                        format!("File is in use: {context}{target}.")
                    }
                    FileAccessKind::ReadOnly => {
                        format!("File is read-only{target}.")
                    }
                    FileAccessKind::NotFound => {
                        format!("File not found: {context}{target}.")
                    }
                    FileAccessKind::Other => {
                        format!("File task failed: {context}{target}.")
                    }
                }
            }
            Self::Win32 { kind, .. } | Self::Platform { kind, .. } => platform_user_message(*kind),
            Self::Dialog { context, code } => {
                format!("Dialog failed: {context} (code {code})")
            }
            Self::Encoding { kind, .. } => match kind {
                EncodingErrorKind::Decode => {
                    "Could not read this file with this encoding. Try another encoding.".to_string()
                }
                EncodingErrorKind::Encode => {
                    "This text cannot be saved with this encoding. Pick another encoding."
                        .to_string()
                }
                EncodingErrorKind::UnsafeText => {
                    "This file has NUL text. It cannot be opened safely.".to_string()
                }
            },
            Self::ExternalFileChanged { path } => format!(
                "File changed outside j3Text ({}). Reload it or use Save As.",
                path.display()
            ),
            Self::FileTooLarge {
                path,
                byte_len,
                limit,
            } => format!(
                "File is too large ({}: {:.1} MB; limit {:.1} MB).",
                path.display(),
                *byte_len as f64 / (1024.0 * 1024.0),
                *limit as f64 / (1024.0 * 1024.0)
            ),
            Self::InvalidState(message) => (*message).to_string(),
        }
    }

    pub fn file_access_kind(&self) -> Option<FileAccessKind> {
        match self {
            Self::Io { kind, .. } => Some(*kind),
            _ => None,
        }
    }

    pub fn encoding_error_kind(&self) -> Option<EncodingErrorKind> {
        match self {
            Self::Encoding { kind, .. } => Some(*kind),
            _ => None,
        }
    }
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                source,
                context,
                path,
                ..
            } => {
                if let Some(path) = path {
                    write!(formatter, "{context} ({}): {source}", path.display())
                } else {
                    write!(formatter, "{context}: {source}")
                }
            }
            Self::Win32 { code, context, .. } => {
                write!(formatter, "{context}: Win32 error {code}")
            }
            Self::Platform {
                context, details, ..
            } => write!(formatter, "{context}: {details}"),
            Self::Dialog { code, context } => write!(formatter, "{context}: dialog error {code}"),
            Self::Encoding { kind, details } => {
                write!(formatter, "encoding {kind:?} failure: {details}")
            }
            Self::ExternalFileChanged { path } => {
                write!(
                    formatter,
                    "file changed outside j3Text ({})",
                    path.display()
                )
            }
            Self::FileTooLarge {
                path,
                byte_len,
                limit,
            } => write!(
                formatter,
                "file too large to open ({}): {byte_len} bytes exceeds {limit} bytes",
                path.display()
            ),
            Self::InvalidState(message) => write!(formatter, "{message}"),
        }
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn classify_io_error(error: &io::Error) -> FileAccessKind {
    match error.raw_os_error() {
        Some(32 | 33) => FileAccessKind::FileInUse,
        _ => match error.kind() {
            io::ErrorKind::PermissionDenied => FileAccessKind::PermissionDenied,
            io::ErrorKind::NotFound => FileAccessKind::NotFound,
            _ => FileAccessKind::Other,
        },
    }
}

fn classify_platform_context(context: &str) -> PlatformErrorKind {
    match context {
        "load Rich Edit library" => PlatformErrorKind::RichEditLibraryUnavailable,
        "create Rich Edit text surface" => PlatformErrorKind::RichEditControlCreateFailed,
        "set Rich Edit plain text mode" => PlatformErrorKind::RichEditPlainTextModeFailed,
        "create main window" => PlatformErrorKind::MainWindowCreateFailed,
        _ => PlatformErrorKind::Other,
    }
}

fn platform_user_message(kind: PlatformErrorKind) -> String {
    match kind {
        PlatformErrorKind::RichEditLibraryUnavailable => concat!(
            "j3Text needs Windows Rich Edit (Msftedit.dll). ",
            "It could not be loaded."
        )
        .to_string(),
        PlatformErrorKind::RichEditControlCreateFailed => concat!(
            "j3Text could not create the text editor. ",
            "Windows Rich Edit may be missing."
        )
        .to_string(),
        PlatformErrorKind::RichEditPlainTextModeFailed => concat!(
            "j3Text could not start plain text mode. ",
            "The app will close."
        )
        .to_string(),
        PlatformErrorKind::MainWindowCreateFailed => {
            "j3Text could not open the main window.".to_string()
        }
        PlatformErrorKind::Other => "A Windows step failed in j3Text.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win32_errors_keep_platform_error_code() {
        let error = AppError::win32("create main window", 1400);

        assert_eq!(
            error.user_message(),
            "j3Text could not open the main window."
        );
        assert_eq!(error.to_string(), "create main window: Win32 error 1400");
    }

    #[test]
    fn encoding_errors_separate_user_message_from_internal_details() {
        let error = AppError::encoding_decode("Invalid UTF-8 BOM input at byte 3");

        assert_eq!(error.encoding_error_kind(), Some(EncodingErrorKind::Decode));
        assert!(error.user_message().contains("this encoding"));
        assert!(!error.user_message().contains("byte 3"));
        assert!(error.to_string().contains("byte 3"));
    }

    #[test]
    fn rich_edit_startup_errors_hide_internal_context_from_user_message() {
        let error = AppError::win32("load Rich Edit library", 126);

        assert!(error.user_message().contains("Msftedit.dll"));
        assert!(!error.user_message().contains("code 126"));
        assert!(error.to_string().contains("Win32 error 126"));
    }

    #[test]
    fn io_errors_can_report_target_path_while_display_keeps_internal_path() {
        let temp_path = PathBuf::from("C:\\Temp\\j3text.tmp");
        let target_path = PathBuf::from("C:\\Users\\me\\draft.txt");
        let error = AppError::io_path_with_user(
            io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
            "create temporary save file",
            temp_path.clone(),
            "save file",
            target_path.clone(),
        );

        assert!(error.user_message().contains("save file"));
        assert!(
            error
                .user_message()
                .contains(&target_path.display().to_string())
        );
        assert!(
            !error
                .user_message()
                .contains(&temp_path.display().to_string())
        );
        assert!(error.to_string().contains("create temporary save file"));
        assert!(error.to_string().contains(&temp_path.display().to_string()));
    }
}
