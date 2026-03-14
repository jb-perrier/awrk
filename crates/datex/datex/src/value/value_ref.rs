use crate::codec::decode::{DecodeConfig, decode_value};
use crate::error::{Result, WireError};

#[derive(Debug, Clone, PartialEq)]
pub enum SerializedValueRef<'a> {
    Null,
    Unit,
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    String(&'a str),
    Bytes(&'a [u8]),
    Array(ArrayRef<'a>),
    Map(MapRef<'a>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Null,
    Unit,
    Bool,
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    String,
    Bytes,
    Array,
    Map,
}

impl<'a> SerializedValueRef<'a> {
    pub fn kind(&self) -> ValueKind {
        match self {
            SerializedValueRef::Null => ValueKind::Null,
            SerializedValueRef::Unit => ValueKind::Unit,
            SerializedValueRef::Bool(_) => ValueKind::Bool,
            SerializedValueRef::U8(_) => ValueKind::U8,
            SerializedValueRef::U16(_) => ValueKind::U16,
            SerializedValueRef::U32(_) => ValueKind::U32,
            SerializedValueRef::U64(_) => ValueKind::U64,
            SerializedValueRef::I8(_) => ValueKind::I8,
            SerializedValueRef::I16(_) => ValueKind::I16,
            SerializedValueRef::I32(_) => ValueKind::I32,
            SerializedValueRef::I64(_) => ValueKind::I64,
            SerializedValueRef::F32(_) => ValueKind::F32,
            SerializedValueRef::F64(_) => ValueKind::F64,
            SerializedValueRef::String(_) => ValueKind::String,
            SerializedValueRef::Bytes(_) => ValueKind::Bytes,
            SerializedValueRef::Array(_) => ValueKind::Array,
            SerializedValueRef::Map(_) => ValueKind::Map,
        }
    }

    pub fn as_str(&self) -> Option<&'a str> {
        match self {
            SerializedValueRef::String(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            SerializedValueRef::U8(v) => Some(*v as u64),
            SerializedValueRef::U16(v) => Some(*v as u64),
            SerializedValueRef::U32(v) => Some(*v as u64),
            SerializedValueRef::U64(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            SerializedValueRef::I8(v) => Some(*v as i64),
            SerializedValueRef::I16(v) => Some(*v as i64),
            SerializedValueRef::I32(v) => Some(*v as i64),
            SerializedValueRef::I64(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&'a [u8]> {
        match self {
            SerializedValueRef::Bytes(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<ArrayRef<'a>> {
        match self {
            SerializedValueRef::Array(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<MapRef<'a>> {
        match self {
            SerializedValueRef::Map(v) => Some(*v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrayRef<'a> {
    pub(crate) payload: &'a [u8],
    pub(crate) count: usize,
    pub(crate) depth: usize,
}

impl<'a> ArrayRef<'a> {
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn iter(&self) -> ArrayIter<'a> {
        ArrayIter {
            payload: self.payload,
            cursor: 0,
            remaining: self.count,
            depth: self.depth,
        }
    }
}

pub struct ArrayIter<'a> {
    payload: &'a [u8],
    cursor: usize,
    remaining: usize,
    depth: usize,
}

impl<'a> ArrayIter<'a> {
    pub fn finish(self) -> Result<()> {
        if self.remaining != 0 {
            return Err(WireError::Malformed("not enough elements in container"));
        }
        if self.cursor != self.payload.len() {
            return Err(WireError::Malformed("trailing bytes in container payload"));
        }
        Ok(())
    }
}

impl<'a> Iterator for ArrayIter<'a> {
    type Item = Result<SerializedValueRef<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let segment = self.payload.get(self.cursor..).unwrap_or(&[]);
        if segment.is_empty() {
            self.remaining = 0;
            return Some(Err(WireError::UnexpectedEof));
        }

        match decode_value(
            segment,
            DecodeConfig {
                max_depth: self.depth,
            },
        ) {
            Ok((value, used)) => {
                if used == 0 {
                    self.remaining = 0;
                    return Some(Err(WireError::Malformed("decoded empty value")));
                }
                self.cursor += used;
                self.remaining -= 1;
                Some(Ok(value))
            }
            Err(e) => {
                self.remaining = 0;
                Some(Err(e))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapRef<'a> {
    pub(crate) payload: &'a [u8],
    pub(crate) count: usize,
    pub(crate) depth: usize,
}

impl<'a> MapRef<'a> {
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn iter_pairs(&self) -> MapIter<'a> {
        MapIter {
            payload: self.payload,
            cursor: 0,
            remaining: self.count,
            depth: self.depth,
        }
    }
}

pub struct MapIter<'a> {
    payload: &'a [u8],
    cursor: usize,
    remaining: usize,
    depth: usize,
}

impl<'a> MapIter<'a> {
    pub fn finish(self) -> Result<()> {
        if self.remaining != 0 {
            return Err(WireError::Malformed("not enough entries in map"));
        }
        if self.cursor != self.payload.len() {
            return Err(WireError::Malformed("trailing bytes in container payload"));
        }
        Ok(())
    }
}

impl<'a> Iterator for MapIter<'a> {
    type Item = Result<(SerializedValueRef<'a>, SerializedValueRef<'a>)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let segment = self.payload.get(self.cursor..).unwrap_or(&[]);
        if segment.is_empty() {
            self.remaining = 0;
            return Some(Err(WireError::UnexpectedEof));
        }

        let (key, used_key) = match decode_value(
            segment,
            DecodeConfig {
                max_depth: self.depth,
            },
        ) {
            Ok(v) => v,
            Err(e) => {
                self.remaining = 0;
                return Some(Err(e));
            }
        };
        let after_key = segment
            .get(used_key..)
            .ok_or(WireError::Malformed("missing value after map key"));
        let after_key = match after_key {
            Ok(v) if !v.is_empty() => v,
            _ => {
                self.remaining = 0;
                return Some(Err(WireError::Malformed("missing value after map key")));
            }
        };

        let (value, used_val) = match decode_value(
            after_key,
            DecodeConfig {
                max_depth: self.depth,
            },
        ) {
            Ok(v) => v,
            Err(e) => {
                self.remaining = 0;
                return Some(Err(e));
            }
        };
        if used_val == 0 {
            self.remaining = 0;
            return Some(Err(WireError::Malformed("decoded empty value")));
        }

        self.cursor += used_key + used_val;
        self.remaining -= 1;
        Some(Ok((key, value)))
    }
}
