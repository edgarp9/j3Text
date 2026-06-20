use crate::error::AppError;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{ERROR_INVALID_FLAGS, ERROR_INVALID_PARAMETER, GetLastError};
#[cfg(windows)]
use windows_sys::Win32::Globalization::{
    MB_ERR_INVALID_CHARS, MultiByteToWideChar, WC_NO_BEST_FIT_CHARS, WideCharToMultiByte,
};

#[cfg(not(windows))]
use encoding_rs::{
    BIG5, EUC_KR, Encoding, GB18030, SHIFT_JIS, WINDOWS_874, WINDOWS_1250, WINDOWS_1251,
    WINDOWS_1252, WINDOWS_1253, WINDOWS_1254, WINDOWS_1255, WINDOWS_1256, WINDOWS_1257,
};

#[cfg(windows)]
#[derive(Default)]
pub(crate) struct CodePageEncodeBuffer {
    wide: Vec<u16>,
    bytes: Vec<u8>,
}

#[cfg(not(windows))]
#[derive(Default)]
pub(crate) struct CodePageEncodeBuffer {
    bytes: Vec<u8>,
}

impl CodePageEncodeBuffer {
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(windows)]
pub(crate) fn decode_code_page(
    bytes: &[u8],
    code_page: u32,
    label: &str,
) -> Result<String, AppError> {
    if bytes.is_empty() {
        return Ok(String::new());
    }

    let input_len: i32 = bytes
        .len()
        .try_into()
        .map_err(|_| AppError::encoding_decode(format!("{label} input is too large")))?;

    // The input slice is valid for input_len bytes and no output buffer is passed
    // during the sizing call.
    let mut required = unsafe {
        MultiByteToWideChar(
            code_page,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            input_len,
            std::ptr::null_mut(),
            0,
        )
    };

    let flags = if required == 0 {
        // Some installed code pages do not accept MB_ERR_INVALID_CHARS. Retry
        // without it only for that documented error.
        let last_error = unsafe { GetLastError() };
        if last_error != ERROR_INVALID_FLAGS {
            return Err(AppError::encoding_decode(format!("Invalid {label} input")));
        }
        // Same input buffer as above; this is another sizing call.
        required = unsafe {
            MultiByteToWideChar(
                code_page,
                0,
                bytes.as_ptr(),
                input_len,
                std::ptr::null_mut(),
                0,
            )
        };
        0
    } else {
        MB_ERR_INVALID_CHARS
    };

    if required == 0 {
        return Err(AppError::encoding_decode(format!("Invalid {label} input")));
    }

    let mut wide = vec![0u16; required as usize];
    // The output buffer has exactly the size requested by the sizing call.
    let written = unsafe {
        MultiByteToWideChar(
            code_page,
            flags,
            bytes.as_ptr(),
            input_len,
            wide.as_mut_ptr(),
            required,
        )
    };

    if written == 0 {
        return Err(AppError::encoding_decode(format!(
            "Failed to decode {label} input"
        )));
    }

    String::from_utf16(&wide[..written as usize])
        .map_err(|_| AppError::encoding_decode(format!("{label} decoded to invalid UTF-16")))
}

#[cfg(not(windows))]
pub(crate) fn decode_code_page(
    bytes: &[u8],
    code_page: u32,
    label: &str,
) -> Result<String, AppError> {
    if bytes.is_empty() {
        return Ok(String::new());
    }
    if code_page == CP_EUC_KR {
        validate_strict_euc_kr_bytes(bytes, label)?;
    }

    let encoding = encoding_for_code_page(code_page)
        .ok_or(AppError::InvalidState("Unsupported code page encoding"))?;
    let (decoded, had_errors) = encoding.decode_without_bom_handling(bytes);
    if had_errors {
        return Err(AppError::encoding_decode(format!("Invalid {label} input")));
    }
    Ok(decoded.into_owned())
}

pub(crate) fn encode_code_page(
    text: &str,
    code_page: u32,
    label: &str,
) -> Result<Vec<u8>, AppError> {
    let mut buffer = CodePageEncodeBuffer::default();
    encode_code_page_reusing(text, code_page, label, &mut buffer)?;
    Ok(buffer.into_bytes())
}

#[cfg(windows)]
pub(crate) fn encode_code_page_reusing<'a>(
    text: &str,
    code_page: u32,
    label: &str,
    buffer: &'a mut CodePageEncodeBuffer,
) -> Result<&'a [u8], AppError> {
    buffer.wide.clear();
    buffer.bytes.clear();

    if text.is_empty() {
        return Ok(&buffer.bytes);
    }

    buffer.wide.extend(text.encode_utf16());
    let input_len: i32 = buffer
        .wide
        .len()
        .try_into()
        .map_err(|_| AppError::encoding_encode(format!("{label} input is too large")))?;

    let mut used_default = 0i32;
    // The wide slice is valid for input_len UTF-16 units; this sizing call has
    // no output buffer and asks Windows to reject best-fit substitutions.
    let required = unsafe {
        WideCharToMultiByte(
            code_page,
            WC_NO_BEST_FIT_CHARS,
            buffer.wide.as_ptr(),
            input_len,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
            &mut used_default,
        )
    };

    if required == 0 {
        let last_error = unsafe { GetLastError() };
        if matches!(last_error, ERROR_INVALID_FLAGS | ERROR_INVALID_PARAMETER) {
            buffer.bytes = encode_code_page_round_trip_checked(
                text,
                &buffer.wide,
                input_len,
                code_page,
                label,
            )?;
            return Ok(&buffer.bytes);
        }
        return Err(AppError::encoding_encode(format!(
            "Text contains characters that cannot be saved as {label}"
        )));
    }
    if used_default != 0 {
        return Err(AppError::encoding_encode(format!(
            "Text contains characters that cannot be saved as {label}"
        )));
    }

    buffer.bytes.resize(required as usize, 0);
    used_default = 0;
    // The byte buffer has the exact capacity returned by the sizing call.
    let written = unsafe {
        WideCharToMultiByte(
            code_page,
            WC_NO_BEST_FIT_CHARS,
            buffer.wide.as_ptr(),
            input_len,
            buffer.bytes.as_mut_ptr(),
            required,
            std::ptr::null(),
            &mut used_default,
        )
    };

    if written == 0 || used_default != 0 {
        return Err(AppError::encoding_encode(format!(
            "Failed to encode text as {label}"
        )));
    }

    buffer.bytes.truncate(written as usize);
    Ok(&buffer.bytes)
}

#[cfg(not(windows))]
pub(crate) fn encode_code_page_reusing<'a>(
    text: &str,
    code_page: u32,
    label: &str,
    buffer: &'a mut CodePageEncodeBuffer,
) -> Result<&'a [u8], AppError> {
    buffer.bytes.clear();

    if text.is_empty() {
        return Ok(&buffer.bytes);
    }

    let encoding = encoding_for_code_page(code_page)
        .ok_or(AppError::InvalidState("Unsupported code page encoding"))?;
    let (encoded, _, had_errors) = encoding.encode(text);
    if had_errors {
        return Err(AppError::encoding_encode(format!(
            "Text contains characters that cannot be saved as {label}"
        )));
    }
    buffer.bytes.extend_from_slice(encoded.as_ref());
    if code_page == CP_EUC_KR {
        validate_strict_euc_kr_encoded_bytes(&buffer.bytes, label)?;
    }
    Ok(&buffer.bytes)
}

#[cfg(windows)]
fn encode_code_page_round_trip_checked(
    text: &str,
    wide: &[u16],
    input_len: i32,
    code_page: u32,
    label: &str,
) -> Result<Vec<u8>, AppError> {
    // Some code pages reject WC_NO_BEST_FIT_CHARS or lpUsedDefaultChar. In that
    // case, accept the encoding only when decoding the produced bytes preserves
    // the exact original text.
    let required = unsafe {
        WideCharToMultiByte(
            code_page,
            0,
            wide.as_ptr(),
            input_len,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
            std::ptr::null_mut(),
        )
    };

    if required == 0 {
        return Err(AppError::encoding_encode(format!(
            "Text contains characters that cannot be saved as {label}"
        )));
    }

    let mut bytes = vec![0u8; required as usize];
    let written = unsafe {
        WideCharToMultiByte(
            code_page,
            0,
            wide.as_ptr(),
            input_len,
            bytes.as_mut_ptr(),
            required,
            std::ptr::null(),
            std::ptr::null_mut(),
        )
    };

    if written == 0 {
        return Err(AppError::encoding_encode(format!(
            "Failed to encode text as {label}"
        )));
    }

    bytes.truncate(written as usize);
    let decoded = decode_code_page(&bytes, code_page, label)?;
    if decoded != text {
        return Err(AppError::encoding_encode(format!(
            "Text contains characters that cannot be saved as {label}"
        )));
    }

    Ok(bytes)
}

#[cfg(not(windows))]
const CP_KOREAN: u32 = 949;
#[cfg(not(windows))]
const CP_EUC_KR: u32 = 51949;
#[cfg(not(windows))]
const CP_SHIFT_JIS: u32 = 932;
#[cfg(not(windows))]
const CP_GB18030: u32 = 54936;
#[cfg(not(windows))]
const CP_BIG5: u32 = 950;
#[cfg(not(windows))]
const CP_WINDOWS_1250: u32 = 1250;
#[cfg(not(windows))]
const CP_WINDOWS_1251: u32 = 1251;
#[cfg(not(windows))]
const CP_WINDOWS_1252: u32 = 1252;
#[cfg(not(windows))]
const CP_WINDOWS_1253: u32 = 1253;
#[cfg(not(windows))]
const CP_WINDOWS_1254: u32 = 1254;
#[cfg(not(windows))]
const CP_WINDOWS_1255: u32 = 1255;
#[cfg(not(windows))]
const CP_WINDOWS_1256: u32 = 1256;
#[cfg(not(windows))]
const CP_WINDOWS_1257: u32 = 1257;
#[cfg(not(windows))]
const CP_WINDOWS_874: u32 = 874;

#[cfg(not(windows))]
fn encoding_for_code_page(code_page: u32) -> Option<&'static Encoding> {
    match code_page {
        CP_KOREAN | CP_EUC_KR => Some(EUC_KR),
        CP_SHIFT_JIS => Some(SHIFT_JIS),
        CP_GB18030 => Some(GB18030),
        CP_BIG5 => Some(BIG5),
        CP_WINDOWS_1250 => Some(WINDOWS_1250),
        CP_WINDOWS_1251 => Some(WINDOWS_1251),
        CP_WINDOWS_1252 => Some(WINDOWS_1252),
        CP_WINDOWS_1253 => Some(WINDOWS_1253),
        CP_WINDOWS_1254 => Some(WINDOWS_1254),
        CP_WINDOWS_1255 => Some(WINDOWS_1255),
        CP_WINDOWS_1256 => Some(WINDOWS_1256),
        CP_WINDOWS_1257 => Some(WINDOWS_1257),
        CP_WINDOWS_874 => Some(WINDOWS_874),
        _ => None,
    }
}

#[cfg(not(windows))]
fn validate_strict_euc_kr_bytes(bytes: &[u8], label: &str) -> Result<(), AppError> {
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            0x00..=0x7f => index += 1,
            0xa1..=0xfe => {
                if !matches!(bytes.get(index + 1), Some(0xa1..=0xfe)) {
                    return Err(AppError::encoding_decode(format!("Invalid {label} input")));
                }
                index += 2;
            }
            _ => return Err(AppError::encoding_decode(format!("Invalid {label} input"))),
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn validate_strict_euc_kr_encoded_bytes(bytes: &[u8], label: &str) -> Result<(), AppError> {
    validate_strict_euc_kr_bytes(bytes, label).map_err(|_| {
        AppError::encoding_encode(format!(
            "Text contains characters that cannot be saved as {label}"
        ))
    })
}
