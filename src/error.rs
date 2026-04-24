use core::fmt;
use std::ffi::CStr;

use crate::sys;
use crate::sys::zvec_error_code_t as raw;

/// Strongly-typed mirror of [`sys::zvec_error_code_t`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    Ok,
    NotFound,
    AlreadyExists,
    InvalidArgument,
    PermissionDenied,
    FailedPrecondition,
    ResourceExhausted,
    Unavailable,
    Internal,
    NotSupported,
    Unknown,
    /// An error code the bindings did not recognise (e.g. from a newer zvec).
    Other(i32),
}

impl ErrorCode {
    pub fn from_raw(code: sys::zvec_error_code_t::Type) -> Self {
        match code {
            raw::ZVEC_OK => ErrorCode::Ok,
            raw::ZVEC_ERROR_NOT_FOUND => ErrorCode::NotFound,
            raw::ZVEC_ERROR_ALREADY_EXISTS => ErrorCode::AlreadyExists,
            raw::ZVEC_ERROR_INVALID_ARGUMENT => ErrorCode::InvalidArgument,
            raw::ZVEC_ERROR_PERMISSION_DENIED => ErrorCode::PermissionDenied,
            raw::ZVEC_ERROR_FAILED_PRECONDITION => ErrorCode::FailedPrecondition,
            raw::ZVEC_ERROR_RESOURCE_EXHAUSTED => ErrorCode::ResourceExhausted,
            raw::ZVEC_ERROR_UNAVAILABLE => ErrorCode::Unavailable,
            raw::ZVEC_ERROR_INTERNAL_ERROR => ErrorCode::Internal,
            raw::ZVEC_ERROR_NOT_SUPPORTED => ErrorCode::NotSupported,
            raw::ZVEC_ERROR_UNKNOWN => ErrorCode::Unknown,
            other => ErrorCode::Other(other as i32),
        }
    }

    pub fn is_ok(self) -> bool {
        matches!(self, ErrorCode::Ok)
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCode::Ok => f.write_str("ok"),
            ErrorCode::NotFound => f.write_str("not found"),
            ErrorCode::AlreadyExists => f.write_str("already exists"),
            ErrorCode::InvalidArgument => f.write_str("invalid argument"),
            ErrorCode::PermissionDenied => f.write_str("permission denied"),
            ErrorCode::FailedPrecondition => f.write_str("failed precondition"),
            ErrorCode::ResourceExhausted => f.write_str("resource exhausted"),
            ErrorCode::Unavailable => f.write_str("unavailable"),
            ErrorCode::Internal => f.write_str("internal error"),
            ErrorCode::NotSupported => f.write_str("not supported"),
            ErrorCode::Unknown => f.write_str("unknown error"),
            ErrorCode::Other(n) => write!(f, "error code {n}"),
        }
    }
}

/// Error returned by fallible safe wrappers over the zvec C API.
///
/// The `message` field, when present, is the last-error message retrieved via
/// [`sys::zvec_get_last_error`] at the time the error was constructed.
#[derive(Debug, Clone)]
pub struct ZvecError {
    pub code: ErrorCode,
    pub message: Option<String>,
}

impl ZvecError {
    /// Construct an error from a raw code, pulling the current thread-local
    /// message out of the C API if one is set.
    pub fn from_code(code: sys::zvec_error_code_t::Type) -> Self {
        let message = unsafe { take_last_error_message() };
        Self {
            code: ErrorCode::from_raw(code),
            message,
        }
    }
}

impl fmt::Display for ZvecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.message {
            Some(msg) => write!(f, "{}: {msg}", self.code),
            None => write!(f, "{}", self.code),
        }
    }
}

impl std::error::Error for ZvecError {}

/// Pulls the last error message out of zvec and frees the underlying buffer.
///
/// # Safety
///
/// Requires that the zvec C API has been linked in. The caller must uphold
/// zvec's contract that `zvec_get_last_error` is safe to call concurrently
/// from any thread (zvec documents it as thread-local).
unsafe fn take_last_error_message() -> Option<String> {
    let mut buf: *mut std::os::raw::c_char = core::ptr::null_mut();
    let code = sys::zvec_get_last_error(&mut buf as *mut _);
    if code != raw::ZVEC_OK || buf.is_null() {
        if !buf.is_null() {
            sys::zvec_free(buf as *mut _);
        }
        return None;
    }
    let msg = CStr::from_ptr(buf).to_string_lossy().into_owned();
    sys::zvec_free(buf as *mut _);
    Some(msg)
}
