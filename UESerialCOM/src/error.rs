use std::os::raw::c_int;
use std::fmt;

/// 错误码定义
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Ok = 0,
    InvalidArgs = 1,
    OpenFailed = 2,
    NotFound = 3,
    WriteFailed = 4,
    ReadFailed = 5,
    TimeoutFailed = 6,
    PermissionDenied = 7,
    IoError = 8,
    JsonParseFailed = 9,
    InvalidHandle = 10,
    BufferOverflow = 11,
    ProtocolError = 12,
    Poisoned = 13,
}

impl ErrorCode {
    pub fn as_c_int(self) -> c_int {
        self as c_int
    }

    pub fn from_c_int(code: c_int) -> Option<Self> {
        match code {
            0 => Some(ErrorCode::Ok),
            1 => Some(ErrorCode::InvalidArgs),
            2 => Some(ErrorCode::OpenFailed),
            3 => Some(ErrorCode::NotFound),
            4 => Some(ErrorCode::WriteFailed),
            5 => Some(ErrorCode::ReadFailed),
            6 => Some(ErrorCode::TimeoutFailed),
            7 => Some(ErrorCode::PermissionDenied),
            8 => Some(ErrorCode::IoError),
            9 => Some(ErrorCode::JsonParseFailed),
            10 => Some(ErrorCode::InvalidHandle),
            11 => Some(ErrorCode::BufferOverflow),
            12 => Some(ErrorCode::ProtocolError),
            13 => Some(ErrorCode::Poisoned),
            _ => None,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCode::Ok => write!(f, "Success"),
            ErrorCode::InvalidArgs => write!(f, "Invalid arguments"),
            ErrorCode::OpenFailed => write!(f, "Failed to open serial port"),
            ErrorCode::NotFound => write!(f, "Serial port not found"),
            ErrorCode::WriteFailed => write!(f, "Write operation failed"),
            ErrorCode::ReadFailed => write!(f, "Read operation failed"),
            ErrorCode::TimeoutFailed => write!(f, "Operation timed out"),
            ErrorCode::PermissionDenied => write!(f, "Permission denied"),
            ErrorCode::IoError => write!(f, "I/O error"),
            ErrorCode::JsonParseFailed => write!(f, "JSON parsing failed"),
            ErrorCode::InvalidHandle => write!(f, "Invalid handle"),
            ErrorCode::BufferOverflow => write!(f, "Buffer overflow"),
            ErrorCode::ProtocolError => write!(f, "Protocol error"),
            ErrorCode::Poisoned => write!(f, "Lock poisoned"),
        }
    }
}

/// 将 serialport 错误映射到错误码
pub fn map_serial_error(err: serialport::Error) -> ErrorCode {
    match err.kind() {
        serialport::ErrorKind::NoDevice => ErrorCode::NotFound,
        serialport::ErrorKind::Io(io_kind) => match io_kind {
            std::io::ErrorKind::TimedOut => ErrorCode::TimeoutFailed,
            std::io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            _ => ErrorCode::IoError,
        },
        _ => ErrorCode::OpenFailed,
    }
}

/// Result 类型别名
pub type Result<T> = std::result::Result<T, ErrorCode>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_conversion() {
        assert_eq!(ErrorCode::Ok.as_c_int(), 0);
        assert_eq!(ErrorCode::InvalidArgs.as_c_int(), 1);
        assert_eq!(ErrorCode::from_c_int(0), Some(ErrorCode::Ok));
        assert_eq!(ErrorCode::from_c_int(999), None);
    }

    #[test]
    fn test_error_display() {
        assert_eq!(ErrorCode::Ok.to_string(), "Success");
        assert_eq!(ErrorCode::InvalidArgs.to_string(), "Invalid arguments");
    }
}
