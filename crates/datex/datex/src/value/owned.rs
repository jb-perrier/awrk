use crate::codec::encode::Encoder;
use crate::error::{Result, WireError};
use crate::traits::{Decode, Encode, Patch, PatchValidate};
use crate::value::SerializedValueRef;

pub type Array = Vec<Value>;
pub type Map = Vec<(Value, Value)>;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
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
    String(String),
    Bytes(Vec<u8>),
    Array(Array),
    Map(Map),
}

impl Value {
    pub fn array(items: Array) -> Self {
        Self::Array(items)
    }

    pub fn map(entries: Map) -> Self {
        Self::Map(entries)
    }

    /// Convenience for building a string-keyed map.
    pub fn object() -> Self {
        Self::Map(Vec::new())
    }

    pub fn as_object(&self) -> Option<&Map> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut Map> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn get_field(&self, key: &str) -> Option<&Value> {
        let Value::Map(entries) = self else {
            return None;
        };

        for (k, v) in entries {
            if matches!(k, Value::String(s) if s == key) {
                return Some(v);
            }
        }
        None
    }

    pub fn get_field_mut(&mut self, key: &str) -> Option<&mut Value> {
        let Value::Map(entries) = self else {
            return None;
        };

        for (k, v) in entries {
            if matches!(k, Value::String(s) if s == key) {
                return Some(v);
            }
        }
        None
    }

    /// Sets a string-keyed field on a map value.
    ///
    /// Returns the previous value if the field existed.
    pub fn set_field(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        let key = key.into();

        let Value::Map(entries) = self else {
            return None;
        };

        for (k, v) in entries.iter_mut() {
            if matches!(k, Value::String(s) if s == &key) {
                let old = std::mem::replace(v, value);
                return Some(old);
            }
        }

        entries.push((Value::String(key), value));
        None
    }

    pub fn remove_field(&mut self, key: &str) -> Option<Value> {
        let Value::Map(entries) = self else {
            return None;
        };

        let idx = entries
            .iter()
            .position(|(k, _)| matches!(k, Value::String(s) if s == key))?;
        Some(entries.remove(idx).1)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut enc = Encoder::new();
        self.encode_into(&mut enc)?;
        Ok(enc.into_inner())
    }

    pub fn encode_into(&self, enc: &mut Encoder) -> Result<()> {
        match self {
            Value::Null => {
                enc.null();
                Ok(())
            }
            Value::Unit => {
                enc.unit();
                Ok(())
            }
            Value::Bool(v) => {
                enc.bool(*v);
                Ok(())
            }
            Value::U8(v) => {
                enc.u8(*v);
                Ok(())
            }
            Value::U16(v) => {
                enc.u16(*v);
                Ok(())
            }
            Value::U32(v) => {
                enc.u32(*v);
                Ok(())
            }
            Value::U64(v) => {
                enc.u64(*v);
                Ok(())
            }
            Value::I8(v) => {
                enc.i8(*v);
                Ok(())
            }
            Value::I16(v) => {
                enc.i16(*v);
                Ok(())
            }
            Value::I32(v) => {
                enc.i32(*v);
                Ok(())
            }
            Value::I64(v) => {
                enc.i64(*v);
                Ok(())
            }
            Value::F32(v) => {
                enc.f32(*v);
                Ok(())
            }
            Value::F64(v) => {
                enc.f64(*v);
                Ok(())
            }
            Value::String(s) => enc.string(s),
            Value::Bytes(b) => enc.bytes(b),
            Value::Array(items) => {
                let count = u32::try_from(items.len()).map_err(|_| WireError::LengthOverflow)?;
                enc.array(count, |w| {
                    for item in items {
                        w.value(|enc| item.encode_into(enc))?;
                    }
                    Ok(())
                })
            }
            Value::Map(entries) => {
                let count = u32::try_from(entries.len()).map_err(|_| WireError::LengthOverflow)?;
                enc.map(count, |w| {
                    for (key, value) in entries {
                        w.entry(|enc| key.encode_into(enc), |enc| value.encode_into(enc))?;
                    }
                    Ok(())
                })
            }
        }
    }
}

impl<'a> TryFrom<SerializedValueRef<'a>> for Value {
    type Error = WireError;

    fn try_from(value: SerializedValueRef<'a>) -> std::result::Result<Self, Self::Error> {
        match value {
            SerializedValueRef::Null => Ok(Value::Null),
            SerializedValueRef::Unit => Ok(Value::Unit),
            SerializedValueRef::Bool(v) => Ok(Value::Bool(v)),
            SerializedValueRef::U8(v) => Ok(Value::U8(v)),
            SerializedValueRef::U16(v) => Ok(Value::U16(v)),
            SerializedValueRef::U32(v) => Ok(Value::U32(v)),
            SerializedValueRef::U64(v) => Ok(Value::U64(v)),
            SerializedValueRef::I8(v) => Ok(Value::I8(v)),
            SerializedValueRef::I16(v) => Ok(Value::I16(v)),
            SerializedValueRef::I32(v) => Ok(Value::I32(v)),
            SerializedValueRef::I64(v) => Ok(Value::I64(v)),
            SerializedValueRef::F32(v) => Ok(Value::F32(v)),
            SerializedValueRef::F64(v) => Ok(Value::F64(v)),
            SerializedValueRef::String(s) => Ok(Value::String(s.to_string())),
            SerializedValueRef::Bytes(b) => Ok(Value::Bytes(b.to_vec())),
            SerializedValueRef::Array(a) => {
                let mut out = Vec::with_capacity(a.len());
                let mut it = a.iter();
                while let Some(entry) = it.next() {
                    out.push(Value::try_from(entry?)?);
                }
                it.finish()?;
                Ok(Value::Array(out))
            }
            SerializedValueRef::Map(m) => {
                let mut out = Vec::with_capacity(m.len());
                let mut it = m.iter_pairs();
                while let Some(entry) = it.next() {
                    let (k, v) = entry?;
                    out.push((Value::try_from(k)?, Value::try_from(v)?));
                }
                it.finish()?;
                Ok(Value::Map(out))
            }
        }
    }
}

impl Encode for Value {
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        self.encode_into(enc)
    }
}

impl<'a> Decode<'a> for Value {
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
        Value::try_from(value).map_err(Into::into)
    }
}

impl Patch for Value {
    fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
        *self = Value::wire_decode(patch)?;
        Ok(())
    }
}

impl PatchValidate for Value {
    fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
        let _ = Value::wire_decode(patch)?;
        Ok(())
    }
}

impl From<()> for Value {
    fn from((): ()) -> Self {
        Value::Unit
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl From<u8> for Value {
    fn from(value: u8) -> Self {
        Value::U8(value)
    }
}

impl From<u16> for Value {
    fn from(value: u16) -> Self {
        Value::U16(value)
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Value::U32(value)
    }
}

impl From<u64> for Value {
    fn from(value: u64) -> Self {
        Value::U64(value)
    }
}

impl From<i8> for Value {
    fn from(value: i8) -> Self {
        Value::I8(value)
    }
}

impl From<i16> for Value {
    fn from(value: i16) -> Self {
        Value::I16(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Value::I32(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Value::I64(value)
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Value::F32(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::F64(value)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::String(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::String(value.to_string())
    }
}

impl From<Vec<u8>> for Value {
    fn from(value: Vec<u8>) -> Self {
        Value::Bytes(value)
    }
}

impl From<&[u8]> for Value {
    fn from(value: &[u8]) -> Self {
        Value::Bytes(value.to_vec())
    }
}
