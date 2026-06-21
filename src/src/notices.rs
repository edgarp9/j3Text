use std::borrow::Cow;
use std::fs;
use std::path::PathBuf;

const ABOUT_TEXT_FILE_NAME: &str = "about.txt";

const EMBEDDED_ABOUT_TEXT: &str = include_str!("../about.txt");

pub(crate) fn about_text() -> Cow<'static, str> {
    read_runtime_notice_file(ABOUT_TEXT_FILE_NAME).unwrap_or(Cow::Borrowed(EMBEDDED_ABOUT_TEXT))
}

fn read_runtime_notice_file(file_name: &str) -> Option<Cow<'static, str>> {
    let path = runtime_notice_path(file_name)?;
    fs::read_to_string(path)
        .ok()
        .filter(|text| !text.trim().is_empty())
        .map(Cow::Owned)
}

fn runtime_notice_path(file_name: &str) -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    Some(executable.parent()?.join(file_name))
}
