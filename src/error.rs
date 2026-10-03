//! Typed public errors. Malformed input is reported without panicking.
use std::fmt;
/// Stable machine-readable error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    InvalidInput,
    InvalidMode,
    InvalidVersion,
    InvalidEcc,
    InvalidEci,
    InvalidGs1,
    InvalidStructuredAppend,
    InvalidColor,
    DataTooLong,
    ResourceLimit,
}
impl ErrorCode {
    /// Language-neutral code used by the command line JSON protocol.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "INVALID_INPUT",
            Self::InvalidMode => "INVALID_MODE",
            Self::InvalidVersion => "INVALID_VERSION",
            Self::InvalidEcc => "INVALID_ECC",
            Self::InvalidEci => "INVALID_ECI",
            Self::InvalidGs1 => "INVALID_GS1",
            Self::InvalidStructuredAppend => "INVALID_STRUCTURED_APPEND",
            Self::InvalidColor => "INVALID_COLOR",
            Self::DataTooLong => "DATA_TOO_LONG",
            Self::ResourceLimit => "RESOURCE_LIMIT",
        }
    }
}
/// An immutable error with a stable category and explanatory message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    code: ErrorCode,
    message: String,
}
impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub const fn kind(&self) -> ErrorCode {
        self.code
    }
    pub const fn code(&self) -> &'static str {
        self.code.as_str()
    }
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.message)
    }
}
impl std::error::Error for Error {}
/// Public fallible API result.
pub type Result<T> = std::result::Result<T, Error>;
