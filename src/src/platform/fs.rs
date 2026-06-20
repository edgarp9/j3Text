#[cfg(target_os = "linux")]
use std::ffi::CString;
#[cfg(not(windows))]
use std::fs;
#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(any(windows, target_os = "linux"))]
use std::io::{self, ErrorKind};
#[cfg(windows)]
use std::mem;
#[cfg(target_os = "linux")]
use std::os::raw::{c_int, c_long, c_uint};
#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStrExt;
#[cfg(all(test, unix))]
use std::os::unix::fs::PermissionsExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::path::Path;

#[cfg(windows)]
use windows_sys::Win32::Foundation::HANDLE;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FileRenameInfoEx, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    SetFileInformationByHandle,
};
#[cfg(all(test, windows))]
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_READONLY, GetFileAttributesW, INVALID_FILE_ATTRIBUTES, SetFileAttributesW,
};

use crate::error::AppError;
#[cfg(any(windows, target_os = "linux"))]
use crate::error::FileAccessKind;

#[cfg(windows)]
const FILE_RENAME_REPLACE_IF_EXISTS: u32 = 0x0000_0001;
#[cfg(windows)]
const FILE_RENAME_POSIX_SEMANTICS: u32 = 0x0000_0002;
#[cfg(target_os = "linux")]
const AT_FDCWD: c_int = -100;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const SYS_RENAMEAT2: c_long = 316;
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const SYS_RENAMEAT2: c_long = 276;
#[cfg(target_os = "linux")]
const RENAME_EXCHANGE: c_uint = 0x2;

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn syscall(num: c_long, ...) -> c_long;
}

#[cfg(windows)]
pub(crate) fn replace_file_atomically(
    temp_path: &Path,
    target_path: &Path,
    target_must_exist: bool,
) -> Result<(), AppError> {
    let temp = path_to_wide(temp_path)?;
    let target = path_to_wide(target_path)?;
    if target_must_exist {
        return replace_existing_file_atomically(temp_path, target_path, target);
    }

    let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;

    // The two buffers are NUL-terminated and live for the duration of this call.
    let ok = unsafe { MoveFileExW(temp.as_ptr(), target.as_ptr(), flags) };
    if ok == 0 {
        return Err(AppError::io_path(
            io::Error::last_os_error(),
            "replace saved file",
            target_path.to_path_buf(),
        ));
    }

    Ok(())
}

#[cfg(target_os = "linux")]
pub(crate) fn replace_file_atomically(
    temp_path: &Path,
    target_path: &Path,
    target_must_exist: bool,
) -> Result<(), AppError> {
    if target_must_exist {
        replace_existing_file_atomically(temp_path, target_path)
    } else {
        fs::rename(temp_path, target_path).map_err(|source| {
            AppError::io_path(source, "replace saved file", target_path.to_path_buf())
        })
    }
}

#[cfg(all(not(windows), not(target_os = "linux")))]
pub(crate) fn replace_file_atomically(
    temp_path: &Path,
    target_path: &Path,
    target_must_exist: bool,
) -> Result<(), AppError> {
    if target_must_exist && !target_path.exists() {
        return Err(AppError::external_file_changed(target_path.to_path_buf()));
    }
    fs::rename(temp_path, target_path).map_err(|source| {
        AppError::io_path(source, "replace saved file", target_path.to_path_buf())
    })
}

#[cfg(target_os = "linux")]
fn replace_existing_file_atomically(temp_path: &Path, target_path: &Path) -> Result<(), AppError> {
    rename_exchange(temp_path, target_path)?;
    fs::remove_file(temp_path).map_err(|source| {
        AppError::io_path(source, "remove replaced file", temp_path.to_path_buf())
    })
}

#[cfg(target_os = "linux")]
fn rename_exchange(temp_path: &Path, target_path: &Path) -> Result<(), AppError> {
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        let temp = path_to_cstring(temp_path, "prepare temporary save path")?;
        let target = path_to_cstring(target_path, "prepare save path")?;
        let result = unsafe {
            syscall(
                SYS_RENAMEAT2,
                AT_FDCWD,
                temp.as_ptr(),
                AT_FDCWD,
                target.as_ptr(),
                RENAME_EXCHANGE,
            )
        };
        if result == 0 {
            return Ok(());
        }
        let source = io::Error::last_os_error();
        if source.kind() == ErrorKind::NotFound {
            Err(AppError::external_file_changed(target_path.to_path_buf()))
        } else {
            Err(AppError::io_path(
                source,
                "replace saved file",
                target_path.to_path_buf(),
            ))
        }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = temp_path;
        Err(AppError::file_access(
            FileAccessKind::Other,
            io::Error::new(
                ErrorKind::Unsupported,
                "renameat2 exchange is not configured for this architecture",
            ),
            "replace saved file",
            Some(target_path.to_path_buf()),
        ))
    }
}

#[cfg(target_os = "linux")]
fn path_to_cstring(path: &Path, context: &'static str) -> Result<CString, AppError> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        AppError::file_access(
            FileAccessKind::Other,
            io::Error::new(ErrorKind::InvalidInput, "path contains NUL"),
            context,
            Some(path.to_path_buf()),
        )
    })
}

#[cfg(windows)]
fn replace_existing_file_atomically(
    temp_path: &Path,
    target_path: &Path,
    mut target: Vec<u16>,
) -> Result<(), AppError> {
    target.truncate(target.len().saturating_sub(1));
    let file_name_bytes = target
        .len()
        .checked_mul(mem::size_of::<u16>())
        .and_then(|len| u32::try_from(len).ok())
        .ok_or_else(|| {
            AppError::file_access(
                FileAccessKind::Other,
                io::Error::new(ErrorKind::InvalidInput, "path is too long"),
                "prepare Windows path",
                Some(target_path.to_path_buf()),
            )
        })?;
    let buffer_len = mem::size_of::<FILE_RENAME_INFO>()
        .checked_add(
            target
                .len()
                .saturating_sub(1)
                .checked_mul(mem::size_of::<u16>())
                .ok_or_else(|| {
                    AppError::file_access(
                        FileAccessKind::Other,
                        io::Error::new(ErrorKind::InvalidInput, "path is too long"),
                        "prepare Windows path",
                        Some(target_path.to_path_buf()),
                    )
                })?,
        )
        .ok_or_else(|| {
            AppError::file_access(
                FileAccessKind::Other,
                io::Error::new(ErrorKind::InvalidInput, "path is too long"),
                "prepare Windows path",
                Some(target_path.to_path_buf()),
            )
        })?;
    let buffer_size = u32::try_from(buffer_len).map_err(|_| {
        AppError::file_access(
            FileAccessKind::Other,
            io::Error::new(ErrorKind::InvalidInput, "path is too long"),
            "prepare Windows path",
            Some(target_path.to_path_buf()),
        )
    })?;
    let buffer_units = buffer_len
        .checked_add(mem::size_of::<FILE_RENAME_INFO>() - 1)
        .map(|len| len / mem::size_of::<FILE_RENAME_INFO>())
        .ok_or_else(|| {
            AppError::file_access(
                FileAccessKind::Other,
                io::Error::new(ErrorKind::InvalidInput, "path is too long"),
                "prepare Windows path",
                Some(target_path.to_path_buf()),
            )
        })?;
    let mut buffer = Vec::with_capacity(buffer_units);
    buffer.resize_with(buffer_units, || {
        // SAFETY: FILE_RENAME_INFO is made of integer, pointer, and UTF-16 code unit
        // fields; zero is a valid initial value before the required fields are written.
        unsafe { mem::zeroed::<FILE_RENAME_INFO>() }
    });
    let rename_info = buffer.as_mut_ptr();
    let temp_file = OpenOptions::new()
        .access_mode(DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(temp_path)
        .map_err(|source| {
            AppError::io_path(source, "open temporary save file", temp_path.to_path_buf())
        })?;

    unsafe {
        // SAFETY: buffer is allocated as FILE_RENAME_INFO units, so rename_info is
        // aligned for header writes and remains alive until SetFileInformationByHandle
        // returns. buffer_size preserves the Win32 byte length for the variable file
        // name payload.
        std::ptr::addr_of_mut!((*rename_info).Anonymous.Flags)
            .write(FILE_RENAME_REPLACE_IF_EXISTS | FILE_RENAME_POSIX_SEMANTICS);
        std::ptr::addr_of_mut!((*rename_info).RootDirectory)
            .write(std::ptr::null_mut::<std::ffi::c_void>() as HANDLE);
        std::ptr::addr_of_mut!((*rename_info).FileNameLength).write(file_name_bytes);
        std::ptr::copy_nonoverlapping(
            target.as_ptr(),
            std::ptr::addr_of_mut!((*rename_info).FileName).cast::<u16>(),
            target.len(),
        );
    }

    let ok = unsafe {
        SetFileInformationByHandle(
            temp_file.as_raw_handle() as HANDLE,
            FileRenameInfoEx,
            rename_info.cast(),
            buffer_size,
        )
    };
    if ok == 0 {
        let source = io::Error::last_os_error();
        if source.kind() == ErrorKind::NotFound {
            return Err(AppError::external_file_changed(target_path.to_path_buf()));
        }
        return Err(AppError::io_path(
            source,
            "replace saved file",
            target_path.to_path_buf(),
        ));
    }

    Ok(())
}

#[cfg(all(test, windows))]
pub(crate) fn clear_readonly_attribute(path: &Path) -> Result<(), AppError> {
    let wide = path_to_wide(path)?;
    let attributes = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if attributes == INVALID_FILE_ATTRIBUTES {
        return Err(AppError::io_path(
            io::Error::last_os_error(),
            "read file attributes",
            path.to_path_buf(),
        ));
    }

    if attributes & FILE_ATTRIBUTE_READONLY == 0 {
        return Ok(());
    }

    let ok = unsafe { SetFileAttributesW(wide.as_ptr(), attributes & !FILE_ATTRIBUTE_READONLY) };
    if ok == 0 {
        return Err(AppError::io_path(
            io::Error::last_os_error(),
            "clear read-only attribute",
            path.to_path_buf(),
        ));
    }

    Ok(())
}

#[cfg(all(test, not(windows)))]
pub(crate) fn clear_readonly_attribute(path: &Path) -> Result<(), AppError> {
    let metadata = fs::metadata(path)
        .map_err(|source| AppError::io_path(source, "read file attributes", path.to_path_buf()))?;
    let mut permissions = metadata.permissions();
    if !permissions.readonly() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        let mode = permissions.mode();
        permissions.set_mode(mode | 0o200);
    }
    #[cfg(not(unix))]
    {
        permissions.set_readonly(false);
    }
    fs::set_permissions(path, permissions).map_err(|source| {
        AppError::io_path(source, "clear read-only attribute", path.to_path_buf())
    })
}

#[cfg(windows)]
fn path_to_wide(path: &Path) -> Result<Vec<u16>, AppError> {
    let mut wide = Vec::new();
    for unit in path.as_os_str().encode_wide() {
        if unit == 0 {
            return Err(AppError::file_access(
                FileAccessKind::Other,
                io::Error::new(ErrorKind::InvalidInput, "path contains NUL"),
                "prepare Windows path",
                Some(path.to_path_buf()),
            ));
        }
        wide.push(unit);
    }
    wide.push(0);
    Ok(wide)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::env;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_TOKEN: AtomicU64 = AtomicU64::new(1);

    struct TempRoot {
        path: std::path::PathBuf,
    }

    impl TempRoot {
        fn new(prefix: &str) -> Self {
            let token = TEST_TOKEN.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!("{prefix}-{token}"));
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
    fn replace_existing_file_atomically_requires_existing_linux_target() {
        let root = TempRoot::new("j3text-linux-replace-missing");
        let temp = root.path().join(".target.txt.tmp");
        let target = root.path().join("target.txt");
        fs::write(&temp, b"new").expect("write temp");

        let error = replace_file_atomically(&temp, &target, true)
            .expect_err("missing target should not be created");

        assert!(matches!(error, AppError::ExternalFileChanged { .. }));
        assert!(!target.exists());
        assert_eq!(fs::read(&temp).expect("temp remains"), b"new");
    }

    #[test]
    fn replace_existing_file_atomically_exchanges_linux_target() {
        let root = TempRoot::new("j3text-linux-replace-existing");
        let temp = root.path().join(".target.txt.tmp");
        let target = root.path().join("target.txt");
        fs::write(&temp, b"new").expect("write temp");
        fs::write(&target, b"old").expect("write target");

        replace_file_atomically(&temp, &target, true).expect("replace existing target");

        assert_eq!(fs::read(&target).expect("read target"), b"new");
        assert!(!temp.exists());
    }
}
