mod encoding;
mod fs;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod win32;

use crate::error::AppError;

pub(crate) use encoding::{
    CodePageEncodeBuffer, decode_code_page, encode_code_page, encode_code_page_reusing,
};
#[cfg(test)]
pub(crate) use fs::clear_readonly_attribute;
pub(crate) use fs::replace_file_atomically;

#[cfg(windows)]
pub(crate) fn last_win32_error(context: &'static str) -> AppError {
    let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
    AppError::win32(context, code)
}

pub fn run() -> Result<(), AppError> {
    #[cfg(windows)]
    {
        win32::run()
    }
    #[cfg(target_os = "linux")]
    {
        linux::run()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(AppError::InvalidState("Unsupported platform."))
    }
}

pub fn report_fatal_startup_error(error: &AppError) {
    #[cfg(windows)]
    {
        win32::report_fatal_startup_error(error);
    }
    #[cfg(target_os = "linux")]
    {
        linux::report_fatal_startup_error(error);
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = error;
    }
}
