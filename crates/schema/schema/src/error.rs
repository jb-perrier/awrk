pub type Result<T> = core::result::Result<T, SchemaError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    LengthOverflow,
    Malformed(&'static str),
    InvalidUtf8,
    UnexpectedEof,
}

impl core::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SchemaError::LengthOverflow => f.write_str("length overflow"),
            SchemaError::Malformed(msg) => write!(f, "malformed schema: {msg}"),
            SchemaError::InvalidUtf8 => f.write_str("invalid utf-8"),
            SchemaError::UnexpectedEof => f.write_str("unexpected eof"),
        }
    }
}

impl std::error::Error for SchemaError {}
