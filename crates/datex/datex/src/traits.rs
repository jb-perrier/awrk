use crate::codec::encode::Encoder;
use crate::error::{Result, WireError};
use crate::value::SerializedValueRef;
use std::any::TypeId;
use std::collections::BTreeMap;

fn vec_t_is_u8<T: 'static>() -> bool {
    TypeId::of::<T>() == TypeId::of::<u8>()
}

fn vec_t_as_u8_slice<T: 'static>(v: &Vec<T>) -> &[u8] {
    debug_assert!(vec_t_is_u8::<T>());
    // Safety: only called when T == u8 (guarded by TypeId check).
    unsafe { core::slice::from_raw_parts(v.as_ptr() as *const u8, v.len()) }
}

fn vec_u8_into_vec_t<T: 'static>(bytes: Vec<u8>) -> Vec<T> {
    debug_assert!(vec_t_is_u8::<T>());
    // Safety: only called when T == u8 (guarded by TypeId check). We reuse the
    // allocation without copying by reinterpreting the raw parts.
    let mut bytes = core::mem::ManuallyDrop::new(bytes);
    unsafe { Vec::from_raw_parts(bytes.as_mut_ptr() as *mut T, bytes.len(), bytes.capacity()) }
}

pub trait Encode {
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()>;
}

pub trait Decode<'a>: Sized {
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self>;
}

pub trait Patch: Sized {
    fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()>;
}

pub trait PatchValidate {
    fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()>;
}

impl<T> Encode for Vec<T>
where
    T: Encode + 'static,
{
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        // Match schema behavior: Vec<u8> is encoded as bytes, not an array of u8.
        if vec_t_is_u8::<T>() {
            return enc.bytes(vec_t_as_u8_slice(self));
        }

        let count = u32::try_from(self.len()).map_err(|_| WireError::LengthOverflow)?;
        enc.array(count, |w| {
            for item in self {
                w.value(|enc| item.wire_encode(enc))?;
            }
            Ok(())
        })
    }
}

impl<'a, T> Decode<'a> for Vec<T>
where
    T: Decode<'a> + 'static,
{
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
        // Match schema behavior: Vec<u8> is decoded from bytes.
        if vec_t_is_u8::<T>() {
            let bytes = match value {
                SerializedValueRef::Bytes(v) => v.to_vec(),
                _ => return Err(WireError::Malformed("expected bytes")),
            };
            return Ok(vec_u8_into_vec_t::<T>(bytes));
        }

        let array = value
            .as_array()
            .ok_or(WireError::Malformed("expected array"))?;

        let mut out = Vec::with_capacity(array.len());
        let mut it = array.iter();
        for entry in it.by_ref() {
            out.push(T::wire_decode(entry?)?);
        }
        it.finish()?;
        Ok(out)
    }
}

impl<T> Patch for Vec<T>
where
    T: for<'b> Decode<'b> + 'static,
{
    fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
        if vec_t_is_u8::<T>() {
            // For patching we intentionally use Vec<T> semantics (array), even though
            // Vec<u8> encodes/decodes as Bytes for compactness.
            if let Some(array) = patch.as_array() {
                let mut next = Vec::with_capacity(array.len());
                let mut it = array.iter();
                for entry in it.by_ref() {
                    let b: u8 = u8::wire_decode(entry?)?;
                    next.push(b);
                }
                it.finish()?;

                *self = vec_u8_into_vec_t::<T>(next);
                return Ok(());
            }

            if let Some(bytes) = patch.as_bytes() {
                *self = vec_u8_into_vec_t::<T>(bytes.to_vec());
                return Ok(());
            }

            return Err(WireError::Malformed("expected array"));
        }

        let array = patch
            .as_array()
            .ok_or(WireError::Malformed("expected array"))?;

        let mut next = Vec::with_capacity(array.len());
        let mut it = array.iter();
        while let Some(entry) = it.next() {
            next.push(T::wire_decode(entry?)?);
        }
        it.finish()?;

        *self = next;
        Ok(())
    }
}

impl<T> PatchValidate for Vec<T>
where
    T: for<'b> Decode<'b> + 'static,
{
    fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
        if vec_t_is_u8::<T>() {
            // Mirror `Patch` behavior: accept array-of-u8 patches, and also accept
            // bytes patches as a convenience.
            if let Some(array) = patch.as_array() {
                let mut it = array.iter();
                while let Some(entry) = it.next() {
                    let _: u8 = u8::wire_decode(entry?)?;
                }
                it.finish()?;
                return Ok(());
            }

            if patch.as_bytes().is_some() {
                return Ok(());
            }

            return Err(WireError::Malformed("expected array"));
        }

        let array = patch
            .as_array()
            .ok_or(WireError::Malformed("expected array"))?;

        let mut it = array.iter();
        while let Some(entry) = it.next() {
            let _: T = T::wire_decode(entry?)?;
        }
        it.finish()?;

        Ok(())
    }
}

impl<K, V> Encode for BTreeMap<K, V>
where
    K: Encode,
    V: Encode,
{
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        let count = u32::try_from(self.len()).map_err(|_| WireError::LengthOverflow)?;
        enc.map(count, |w| {
            for (k, v) in self {
                w.entry(|enc| k.wire_encode(enc), |enc| v.wire_encode(enc))?;
            }
            Ok(())
        })
    }
}

impl<'a, K, V> Decode<'a> for BTreeMap<K, V>
where
    K: Decode<'a> + Ord,
    V: Decode<'a>,
{
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
        let map = value.as_map().ok_or(WireError::Malformed("expected map"))?;

        let mut out = BTreeMap::new();
        let mut it = map.iter_pairs();
        while let Some(entry) = it.next() {
            let (k, v) = entry?;
            out.insert(K::wire_decode(k)?, V::wire_decode(v)?);
        }
        it.finish()?;
        Ok(out)
    }
}

impl<K, V> Patch for BTreeMap<K, V>
where
    K: for<'a> Decode<'a> + Ord,
    V: for<'a> Decode<'a>,
{
    fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
        *self = <BTreeMap<K, V> as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl<K, V> PatchValidate for BTreeMap<K, V>
where
    K: for<'a> Decode<'a> + Ord,
    V: for<'a> Decode<'a>,
{
    fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
        let _ = <BTreeMap<K, V> as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl Encode for SerializedValueRef<'_> {
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        enc.value_ref(self)
    }
}

impl<'a> Decode<'a> for SerializedValueRef<'a> {
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
        Ok(value)
    }
}

impl Encode for () {
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        enc.unit();
        Ok(())
    }
}

impl<'a> Decode<'a> for () {
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
        match value {
            SerializedValueRef::Unit => Ok(()),
            _ => Err(WireError::Malformed("expected unit")),
        }
    }
}

impl Patch for () {
    fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
        let _ = <() as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl PatchValidate for () {
    fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
        let _ = <() as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl<T> Encode for Option<T>
where
    T: Encode,
{
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        match self {
            None => {
                enc.null();
                Ok(())
            }
            Some(v) => v.wire_encode(enc),
        }
    }
}

impl<'a, T> Decode<'a> for Option<T>
where
    T: Decode<'a>,
{
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
        match value {
            SerializedValueRef::Null => Ok(None),
            other => Ok(Some(T::wire_decode(other)?)),
        }
    }
}

impl<T> Patch for Option<T>
where
    T: Patch + for<'b> Decode<'b>,
{
    fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
        match patch {
            SerializedValueRef::Null => {
                *self = None;
                Ok(())
            }
            other => {
                if let Some(v) = self.as_mut() {
                    v.wire_patch(other)
                } else {
                    *self = Some(T::wire_decode(other)?);
                    Ok(())
                }
            }
        }
    }
}

impl<T> PatchValidate for Option<T>
where
    T: PatchValidate + for<'b> Decode<'b>,
{
    fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
        match patch {
            SerializedValueRef::Null => Ok(()),
            other => {
                if let Some(v) = self.as_ref() {
                    v.wire_patch_validate(other)
                } else {
                    let _: T = T::wire_decode(other)?;
                    Ok(())
                }
            }
        }
    }
}

impl Encode for bool {
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        enc.bool(*self);
        Ok(())
    }
}

impl<'a> Decode<'a> for bool {
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
        match value {
            SerializedValueRef::Bool(v) => Ok(v),
            _ => Err(WireError::Malformed("expected bool")),
        }
    }
}

impl Patch for bool {
    fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
        *self = <bool as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl PatchValidate for bool {
    fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
        let _ = <bool as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

macro_rules! impl_unsigned {
    ($t:ty, $max:expr, $enc_method:ident, $msg:literal) => {
        impl Encode for $t {
            fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
                enc.$enc_method(*self as _);
                Ok(())
            }
        }

        impl<'a> Decode<'a> for $t {
            fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
                let v = value.as_u64().ok_or(WireError::Malformed($msg))?;
                if v > $max {
                    return Err(WireError::Malformed("integer out of range"));
                }
                Ok(v as $t)
            }
        }

        impl Patch for $t {
            fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
                *self = <$t as Decode>::wire_decode(patch)?;
                Ok(())
            }
        }

        impl PatchValidate for $t {
            fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
                let _ = <$t as Decode>::wire_decode(patch)?;
                Ok(())
            }
        }
    };
}

macro_rules! impl_signed {
    ($t:ty, $min:expr, $max:expr, $enc_method:ident, $msg:literal) => {
        impl Encode for $t {
            fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
                enc.$enc_method(*self as _);
                Ok(())
            }
        }

        impl<'a> Decode<'a> for $t {
            fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
                let v = value.as_i64().ok_or(WireError::Malformed($msg))?;
                if v < $min || v > $max {
                    return Err(WireError::Malformed("integer out of range"));
                }
                Ok(v as $t)
            }
        }

        impl Patch for $t {
            fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
                *self = <$t as Decode>::wire_decode(patch)?;
                Ok(())
            }
        }

        impl PatchValidate for $t {
            fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
                let _ = <$t as Decode>::wire_decode(patch)?;
                Ok(())
            }
        }
    };
}

impl_unsigned!(u8, u8::MAX as u64, u8, "expected unsigned int");
impl_unsigned!(u16, u16::MAX as u64, u16, "expected unsigned int");
impl_unsigned!(u32, u32::MAX as u64, u32, "expected unsigned int");

impl Encode for u64 {
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        enc.u64(*self);
        Ok(())
    }
}

impl<'a> Decode<'a> for u64 {
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
        value
            .as_u64()
            .ok_or(WireError::Malformed("expected unsigned int"))
    }
}

impl Patch for u64 {
    fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
        *self = <u64 as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl PatchValidate for u64 {
    fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
        let _ = <u64 as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl_signed!(
    i8,
    i8::MIN as i64,
    i8::MAX as i64,
    i8,
    "expected signed int"
);
impl_signed!(
    i16,
    i16::MIN as i64,
    i16::MAX as i64,
    i16,
    "expected signed int"
);
impl_signed!(
    i32,
    i32::MIN as i64,
    i32::MAX as i64,
    i32,
    "expected signed int"
);

impl Encode for i64 {
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        enc.i64(*self);
        Ok(())
    }
}

impl<'a> Decode<'a> for i64 {
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
        value
            .as_i64()
            .ok_or(WireError::Malformed("expected signed int"))
    }
}

impl Patch for i64 {
    fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
        *self = <i64 as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl PatchValidate for i64 {
    fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
        let _ = <i64 as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl Encode for f32 {
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        enc.f32(*self);
        Ok(())
    }
}

impl<'a> Decode<'a> for f32 {
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
        match value {
            SerializedValueRef::F32(v) => Ok(v),
            _ => Err(WireError::Malformed("expected f32")),
        }
    }
}

impl Patch for f32 {
    fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
        *self = <f32 as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl PatchValidate for f32 {
    fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
        let _ = <f32 as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl Encode for f64 {
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        enc.f64(*self);
        Ok(())
    }
}

impl<'a> Decode<'a> for f64 {
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
        match value {
            SerializedValueRef::F64(v) => Ok(v),
            _ => Err(WireError::Malformed("expected f64")),
        }
    }
}

impl Patch for f64 {
    fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
        *self = <f64 as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl PatchValidate for f64 {
    fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
        let _ = <f64 as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl Encode for str {
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        enc.string(self)
    }
}

impl Encode for String {
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        enc.string(self)
    }
}

impl<'a> Decode<'a> for String {
    fn wire_decode(value: SerializedValueRef<'a>) -> Result<Self> {
        match value {
            SerializedValueRef::String(v) => Ok(v.to_owned()),
            _ => Err(WireError::Malformed("expected string")),
        }
    }
}

impl Patch for String {
    fn wire_patch<'a>(&mut self, patch: SerializedValueRef<'a>) -> Result<()> {
        *self = <String as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl PatchValidate for String {
    fn wire_patch_validate<'a>(&self, patch: SerializedValueRef<'a>) -> Result<()> {
        let _ = <String as Decode>::wire_decode(patch)?;
        Ok(())
    }
}

impl Encode for [u8] {
    fn wire_encode(&self, enc: &mut Encoder) -> Result<()> {
        enc.bytes(self)
    }
}
