use core::fmt;

pub type Result<T> = core::result::Result<T, WireError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    UnexpectedEof,
    InvalidTag(u8),
    InvalidUtf8,
    LengthOverflow,
    RecursionLimitExceeded,
    Malformed(&'static str),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::UnexpectedEof => write!(f, "unexpected end of frame"),
            WireError::InvalidTag(tag) => write!(f, "invalid wire tag: {tag:#x}"),
            WireError::InvalidUtf8 => write!(f, "invalid utf-8 string data"),
            WireError::LengthOverflow => write!(f, "length overflows frame bounds"),
            WireError::RecursionLimitExceeded => write!(f, "value nesting limit exceeded"),
            WireError::Malformed(message) => write!(f, "malformed frame: {message}"),
        }
    }
}

impl std::error::Error for WireError {}
