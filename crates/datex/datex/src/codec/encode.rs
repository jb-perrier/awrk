use crate::codec::tags;
use crate::error::{Result, WireError};
use crate::value::{ArrayRef, MapRef, SerializedValueRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LenWidth {
    W8,
    W16,
    W32,
}

fn choose_len_width(len: usize) -> Result<LenWidth> {
    if len <= u8::MAX as usize {
        Ok(LenWidth::W8)
    } else if len <= u16::MAX as usize {
        Ok(LenWidth::W16)
    } else {
        u32::try_from(len).map_err(|_| WireError::LengthOverflow)?;
        Ok(LenWidth::W32)
    }
}

fn choose_container_width(count: u32, payload_len: usize) -> Result<LenWidth> {
    if count <= u8::MAX as u32 && payload_len <= u8::MAX as usize {
        Ok(LenWidth::W8)
    } else if count <= u16::MAX as u32 && payload_len <= u16::MAX as usize {
        Ok(LenWidth::W16)
    } else {
        u32::try_from(payload_len).map_err(|_| WireError::LengthOverflow)?;
        Ok(LenWidth::W32)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EncodeConfig {
    pub max_depth: usize,
    pub compact_ints: bool,
}

impl Default for EncodeConfig {
    fn default() -> Self {
        Self {
            max_depth: 64,
            compact_ints: true,
        }
    }
}

#[derive(Debug, Default)]
pub struct Encoder {
    buf: Vec<u8>,
    config: EncodeConfig,
}

impl Encoder {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            config: EncodeConfig::default(),
        }
    }

    pub fn with_config(config: EncodeConfig) -> Self {
        Self {
            buf: Vec::new(),
            config,
        }
    }

    pub fn from_vec(buf: Vec<u8>, config: EncodeConfig) -> Self {
        Self { buf, config }
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.buf
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }

    pub fn reserve(&mut self, additional: usize) {
        self.buf.reserve(additional);
    }

    pub fn value_ref(&mut self, value: &SerializedValueRef<'_>) -> Result<()> {
        self.value_ref_at_depth(value, self.config.max_depth)
    }

    fn value_ref_at_depth(&mut self, value: &SerializedValueRef<'_>, depth: usize) -> Result<()> {
        if depth == 0 {
            return Err(WireError::RecursionLimitExceeded);
        }

        match value {
            SerializedValueRef::Null => self.buf.push(tags::TAG_NULL),
            SerializedValueRef::Unit => self.buf.push(tags::TAG_UNIT),
            SerializedValueRef::Bool(false) => self.buf.push(tags::TAG_BOOL_FALSE),
            SerializedValueRef::Bool(true) => self.buf.push(tags::TAG_BOOL_TRUE),
            SerializedValueRef::U8(v) => {
                self.buf.push(tags::TAG_U8);
                self.buf.push(*v);
            }
            SerializedValueRef::U16(v) => {
                self.buf.push(tags::TAG_U16);
                self.buf.extend_from_slice(&v.to_be_bytes());
            }
            SerializedValueRef::U32(v) => {
                self.buf.push(tags::TAG_U32);
                self.buf.extend_from_slice(&v.to_be_bytes());
            }
            SerializedValueRef::U64(v) => {
                self.buf.push(tags::TAG_U64);
                self.buf.extend_from_slice(&v.to_be_bytes());
            }
            SerializedValueRef::I8(v) => {
                self.buf.push(tags::TAG_I8);
                self.buf.push(v.to_be_bytes()[0]);
            }
            SerializedValueRef::I16(v) => {
                self.buf.push(tags::TAG_I16);
                self.buf.extend_from_slice(&v.to_be_bytes());
            }
            SerializedValueRef::I32(v) => {
                self.buf.push(tags::TAG_I32);
                self.buf.extend_from_slice(&v.to_be_bytes());
            }
            SerializedValueRef::I64(v) => {
                self.buf.push(tags::TAG_I64);
                self.buf.extend_from_slice(&v.to_be_bytes());
            }
            SerializedValueRef::F32(v) => {
                self.buf.push(tags::TAG_F32);
                self.buf.extend_from_slice(&v.to_be_bytes());
            }
            SerializedValueRef::F64(v) => {
                self.buf.push(tags::TAG_F64);
                self.buf.extend_from_slice(&v.to_be_bytes());
            }
            SerializedValueRef::String(s) => self.string(s)?,
            SerializedValueRef::Bytes(b) => self.bytes(b)?,
            SerializedValueRef::Array(a) => self.array_raw(*a)?,
            SerializedValueRef::Map(m) => self.map_raw(*m)?,
        }

        Ok(())
    }

    pub fn null(&mut self) {
        self.buf.push(tags::TAG_NULL);
    }

    pub fn unit(&mut self) {
        self.buf.push(tags::TAG_UNIT);
    }

    pub fn bool(&mut self, v: bool) {
        self.buf.push(if v {
            tags::TAG_BOOL_TRUE
        } else {
            tags::TAG_BOOL_FALSE
        });
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(tags::TAG_U8);
        self.buf.push(v);
    }

    pub fn u16(&mut self, v: u16) {
        if self.config.compact_ints && v <= u8::MAX as u16 {
            self.u8(v as u8);
            return;
        }

        self.buf.push(tags::TAG_U16);
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        if self.config.compact_ints {
            if v <= u8::MAX as u32 {
                self.u8(v as u8);
                return;
            }
            if v <= u16::MAX as u32 {
                self.u16(v as u16);
                return;
            }
        }

        self.buf.push(tags::TAG_U32);
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        if self.config.compact_ints {
            if v <= u8::MAX as u64 {
                self.u8(v as u8);
                return;
            }
            if v <= u16::MAX as u64 {
                self.u16(v as u16);
                return;
            }
            if v <= u32::MAX as u64 {
                self.u32(v as u32);
                return;
            }
        }

        self.buf.push(tags::TAG_U64);
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i8(&mut self, v: i8) {
        self.buf.push(tags::TAG_I8);
        self.buf.push(v.to_be_bytes()[0]);
    }

    pub fn i16(&mut self, v: i16) {
        if self.config.compact_ints && (i8::MIN as i16..=i8::MAX as i16).contains(&v) {
            self.i8(v as i8);
            return;
        }

        self.buf.push(tags::TAG_I16);
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        if self.config.compact_ints {
            if (i8::MIN as i32..=i8::MAX as i32).contains(&v) {
                self.i8(v as i8);
                return;
            }
            if (i16::MIN as i32..=i16::MAX as i32).contains(&v) {
                self.i16(v as i16);
                return;
            }
        }

        self.buf.push(tags::TAG_I32);
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        if self.config.compact_ints {
            if (i8::MIN as i64..=i8::MAX as i64).contains(&v) {
                self.i8(v as i8);
                return;
            }
            if (i16::MIN as i64..=i16::MAX as i64).contains(&v) {
                self.i16(v as i16);
                return;
            }
            if (i32::MIN as i64..=i32::MAX as i64).contains(&v) {
                self.i32(v as i32);
                return;
            }
        }

        self.buf.push(tags::TAG_I64);
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn f32(&mut self, v: f32) {
        self.buf.push(tags::TAG_F32);
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        self.buf.push(tags::TAG_F64);
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn string(&mut self, s: &str) -> Result<()> {
        let len = s.len();
        match choose_len_width(len)? {
            LenWidth::W8 => {
                self.buf.push(tags::TAG_STRING_LEN8);
                self.buf.push(u8::try_from(len).unwrap());
            }
            LenWidth::W16 => {
                self.buf.push(tags::TAG_STRING_LEN16);
                let len = u16::try_from(len).unwrap();
                self.buf.extend_from_slice(&len.to_be_bytes());
            }
            LenWidth::W32 => {
                self.buf.push(tags::TAG_STRING);
                let len = u32::try_from(len).unwrap();
                self.buf.extend_from_slice(&len.to_be_bytes());
            }
        }
        self.buf.extend_from_slice(s.as_bytes());
        Ok(())
    }

    pub fn bytes(&mut self, b: &[u8]) -> Result<()> {
        let len = b.len();
        match choose_len_width(len)? {
            LenWidth::W8 => {
                self.buf.push(tags::TAG_BYTES_LEN8);
                self.buf.push(u8::try_from(len).unwrap());
            }
            LenWidth::W16 => {
                self.buf.push(tags::TAG_BYTES_LEN16);
                let len = u16::try_from(len).unwrap();
                self.buf.extend_from_slice(&len.to_be_bytes());
            }
            LenWidth::W32 => {
                self.buf.push(tags::TAG_BYTES);
                let len = u32::try_from(len).unwrap();
                self.buf.extend_from_slice(&len.to_be_bytes());
            }
        }
        self.buf.extend_from_slice(b);
        Ok(())
    }

    pub fn array<F>(&mut self, count: u32, f: F) -> Result<()>
    where
        F: FnOnce(&mut SeqWriter<'_>) -> Result<()>,
    {
        let tag_pos = self.buf.len();
        self.buf.push(tags::TAG_ARRAY);
        self.array_like(tag_pos, count, f)
    }

    pub fn map<F>(&mut self, count: u32, f: F) -> Result<()>
    where
        F: FnOnce(&mut MapWriter<'_>) -> Result<()>,
    {
        let tag_pos = self.buf.len();
        self.buf.push(tags::TAG_MAP);
        self.map_like(tag_pos, count, f)
    }

    fn array_like<F>(&mut self, tag_pos: usize, count: u32, f: F) -> Result<()>
    where
        F: FnOnce(&mut SeqWriter<'_>) -> Result<()>,
    {
        debug_assert_eq!(tag_pos + 1, self.buf.len());

        self.buf.extend_from_slice(&count.to_be_bytes());

        let payload_len_pos = self.buf.len();
        self.buf.extend_from_slice(&0u32.to_be_bytes());
        let payload_start = self.buf.len();

        let mut w = SeqWriter {
            enc: self,
            tag_pos,
            count,
            payload_len_pos,
            payload_start,
            next: 0,
        };
        f(&mut w)?;
        w.finish()
    }

    fn map_like<F>(&mut self, tag_pos: usize, count: u32, f: F) -> Result<()>
    where
        F: FnOnce(&mut MapWriter<'_>) -> Result<()>,
    {
        debug_assert_eq!(tag_pos + 1, self.buf.len());

        self.buf.extend_from_slice(&count.to_be_bytes());

        let payload_len_pos = self.buf.len();
        self.buf.extend_from_slice(&0u32.to_be_bytes());
        let payload_start = self.buf.len();

        let mut w = MapWriter {
            enc: self,
            tag_pos,
            count,
            payload_len_pos,
            payload_start,
            next: 0,
            seen_keys: Vec::with_capacity(count as usize),
        };
        f(&mut w)?;
        w.finish()
    }

    fn array_raw(&mut self, a: ArrayRef<'_>) -> Result<()> {
        let count = u32::try_from(a.count).map_err(|_| WireError::LengthOverflow)?;
        let payload_len = a.payload.len();
        match choose_container_width(count, payload_len)? {
            LenWidth::W8 => {
                self.buf.push(tags::TAG_ARRAY_LEN8);
                self.buf.push(u8::try_from(count).unwrap());
                self.buf.push(u8::try_from(payload_len).unwrap());
            }
            LenWidth::W16 => {
                self.buf.push(tags::TAG_ARRAY_LEN16);
                let c = u16::try_from(count).unwrap();
                let l = u16::try_from(payload_len).unwrap();
                self.buf.extend_from_slice(&c.to_be_bytes());
                self.buf.extend_from_slice(&l.to_be_bytes());
            }
            LenWidth::W32 => {
                self.buf.push(tags::TAG_ARRAY);
                self.buf.extend_from_slice(&count.to_be_bytes());
                let payload_len = u32::try_from(payload_len).unwrap();
                self.buf.extend_from_slice(&payload_len.to_be_bytes());
            }
        }
        self.buf.extend_from_slice(a.payload);
        Ok(())
    }

    fn map_raw(&mut self, m: MapRef<'_>) -> Result<()> {
        let count = u32::try_from(m.count).map_err(|_| WireError::LengthOverflow)?;
        let payload_len = m.payload.len();
        match choose_container_width(count, payload_len)? {
            LenWidth::W8 => {
                self.buf.push(tags::TAG_MAP_LEN8);
                self.buf.push(u8::try_from(count).unwrap());
                self.buf.push(u8::try_from(payload_len).unwrap());
            }
            LenWidth::W16 => {
                self.buf.push(tags::TAG_MAP_LEN16);
                let c = u16::try_from(count).unwrap();
                let l = u16::try_from(payload_len).unwrap();
                self.buf.extend_from_slice(&c.to_be_bytes());
                self.buf.extend_from_slice(&l.to_be_bytes());
            }
            LenWidth::W32 => {
                self.buf.push(tags::TAG_MAP);
                self.buf.extend_from_slice(&count.to_be_bytes());
                let payload_len = u32::try_from(payload_len).unwrap();
                self.buf.extend_from_slice(&payload_len.to_be_bytes());
            }
        }
        self.buf.extend_from_slice(m.payload);
        Ok(())
    }
}

pub struct SeqWriter<'a> {
    enc: &'a mut Encoder,
    tag_pos: usize,
    count: u32,
    payload_len_pos: usize,
    payload_start: usize,
    next: u32,
}

impl<'a> SeqWriter<'a> {
    pub fn bool(&mut self, v: bool) -> Result<()> {
        self.value(|enc| {
            enc.bool(v);
            Ok(())
        })
    }

    pub fn u8(&mut self, v: u8) -> Result<()> {
        self.value(|enc| {
            enc.u8(v);
            Ok(())
        })
    }

    pub fn u16(&mut self, v: u16) -> Result<()> {
        self.value(|enc| {
            enc.u16(v);
            Ok(())
        })
    }

    pub fn u32(&mut self, v: u32) -> Result<()> {
        self.value(|enc| {
            enc.u32(v);
            Ok(())
        })
    }

    pub fn u64(&mut self, v: u64) -> Result<()> {
        self.value(|enc| {
            enc.u64(v);
            Ok(())
        })
    }

    pub fn i8(&mut self, v: i8) -> Result<()> {
        self.value(|enc| {
            enc.i8(v);
            Ok(())
        })
    }

    pub fn i16(&mut self, v: i16) -> Result<()> {
        self.value(|enc| {
            enc.i16(v);
            Ok(())
        })
    }

    pub fn i32(&mut self, v: i32) -> Result<()> {
        self.value(|enc| {
            enc.i32(v);
            Ok(())
        })
    }

    pub fn i64(&mut self, v: i64) -> Result<()> {
        self.value(|enc| {
            enc.i64(v);
            Ok(())
        })
    }

    pub fn f32(&mut self, v: f32) -> Result<()> {
        self.value(|enc| {
            enc.f32(v);
            Ok(())
        })
    }

    pub fn f64(&mut self, v: f64) -> Result<()> {
        self.value(|enc| {
            enc.f64(v);
            Ok(())
        })
    }

    pub fn string(&mut self, s: &str) -> Result<()> {
        self.value(|enc| enc.string(s))
    }

    pub fn bytes(&mut self, b: &[u8]) -> Result<()> {
        self.value(|enc| enc.bytes(b))
    }

    pub fn value<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut Encoder) -> Result<()>,
    {
        if self.next >= self.count {
            return Err(WireError::Malformed("too many elements in container"));
        }

        let before = self.enc.buf.len();
        f(self.enc)?;
        if self.enc.buf.len() == before {
            return Err(WireError::Malformed("container element is empty"));
        }

        self.next += 1;
        Ok(())
    }

    fn finish(self) -> Result<()> {
        if self.next != self.count {
            return Err(WireError::Malformed("not enough elements in container"));
        }

        let payload_len = self
            .enc
            .buf
            .len()
            .checked_sub(self.payload_start)
            .ok_or(WireError::LengthOverflow)?;
        let width = choose_container_width(self.count, payload_len)?;

        match width {
            LenWidth::W32 => {
                let payload_len_u32 =
                    u32::try_from(payload_len).map_err(|_| WireError::LengthOverflow)?;
                self.enc.buf[self.payload_len_pos..self.payload_len_pos + 4]
                    .copy_from_slice(&payload_len_u32.to_be_bytes());
                Ok(())
            }
            LenWidth::W16 => {
                self.enc.buf[self.tag_pos] = tags::TAG_ARRAY_LEN16;
                let count = u16::try_from(self.count).unwrap();
                let payload_len = u16::try_from(payload_len).unwrap();

                let count_pos = self.tag_pos + 1;
                self.enc.buf[count_pos..count_pos + 2].copy_from_slice(&count.to_be_bytes());
                self.enc.buf[count_pos + 2..count_pos + 4]
                    .copy_from_slice(&payload_len.to_be_bytes());

                let new_payload_start = self.tag_pos + 1 + 2 + 2;
                let end = self.enc.buf.len();
                let delta = self
                    .payload_start
                    .checked_sub(new_payload_start)
                    .ok_or(WireError::LengthOverflow)?;
                debug_assert_eq!(delta, 4);
                self.enc
                    .buf
                    .copy_within(self.payload_start..end, new_payload_start);
                self.enc.buf.truncate(end - delta);
                Ok(())
            }
            LenWidth::W8 => {
                self.enc.buf[self.tag_pos] = tags::TAG_ARRAY_LEN8;
                let count = u8::try_from(self.count).unwrap();
                let payload_len = u8::try_from(payload_len).unwrap();

                let count_pos = self.tag_pos + 1;
                self.enc.buf[count_pos] = count;
                self.enc.buf[count_pos + 1] = payload_len;

                let new_payload_start = self.tag_pos + 1 + 1 + 1;
                let end = self.enc.buf.len();
                let delta = self
                    .payload_start
                    .checked_sub(new_payload_start)
                    .ok_or(WireError::LengthOverflow)?;
                debug_assert_eq!(delta, 6);
                self.enc
                    .buf
                    .copy_within(self.payload_start..end, new_payload_start);
                self.enc.buf.truncate(end - delta);
                Ok(())
            }
        }
    }
}

pub struct MapWriter<'a> {
    enc: &'a mut Encoder,
    tag_pos: usize,
    count: u32,
    payload_len_pos: usize,
    payload_start: usize,
    next: u32,
    seen_keys: Vec<(u64, usize, usize)>,
}

impl<'a> MapWriter<'a> {
    pub fn entry<K, V>(&mut self, key: K, value: V) -> Result<()>
    where
        K: FnOnce(&mut Encoder) -> Result<()>,
        V: FnOnce(&mut Encoder) -> Result<()>,
    {
        if self.next >= self.count {
            return Err(WireError::Malformed("too many entries in map"));
        }

        let before = self.enc.buf.len();
        let key_start = self.enc.buf.len();
        key(self.enc)?;
        let key_end = self.enc.buf.len();
        if key_end == key_start {
            return Err(WireError::Malformed("map entry key is empty"));
        }

        let new_key = &self.enc.buf[key_start..key_end];
        let new_hash = fnv1a64(new_key);
        let is_duplicate = self
            .seen_keys
            .iter()
            .any(|(hash, start, end)| *hash == new_hash && &self.enc.buf[*start..*end] == new_key);
        if is_duplicate {
            self.enc.buf.truncate(before);
            return Err(WireError::Malformed("duplicate map key"));
        }
        self.seen_keys.push((new_hash, key_start, key_end));

        let before_val = self.enc.buf.len();
        value(self.enc)?;
        if self.enc.buf.len() == before_val {
            return Err(WireError::Malformed("map entry value is empty"));
        }

        if self.enc.buf.len() == before {
            return Err(WireError::Malformed("map entry is empty"));
        }

        self.next += 1;
        Ok(())
    }

    fn finish(self) -> Result<()> {
        if self.next != self.count {
            return Err(WireError::Malformed("not enough entries in map"));
        }
        let payload_len = self
            .enc
            .buf
            .len()
            .checked_sub(self.payload_start)
            .ok_or(WireError::LengthOverflow)?;
        let width = choose_container_width(self.count, payload_len)?;

        match width {
            LenWidth::W32 => {
                let payload_len_u32 =
                    u32::try_from(payload_len).map_err(|_| WireError::LengthOverflow)?;
                self.enc.buf[self.payload_len_pos..self.payload_len_pos + 4]
                    .copy_from_slice(&payload_len_u32.to_be_bytes());
                Ok(())
            }
            LenWidth::W16 => {
                self.enc.buf[self.tag_pos] = tags::TAG_MAP_LEN16;
                let count = u16::try_from(self.count).unwrap();
                let payload_len = u16::try_from(payload_len).unwrap();

                let count_pos = self.tag_pos + 1;
                self.enc.buf[count_pos..count_pos + 2].copy_from_slice(&count.to_be_bytes());
                self.enc.buf[count_pos + 2..count_pos + 4]
                    .copy_from_slice(&payload_len.to_be_bytes());

                let new_payload_start = self.tag_pos + 1 + 2 + 2;
                let end = self.enc.buf.len();
                let delta = self
                    .payload_start
                    .checked_sub(new_payload_start)
                    .ok_or(WireError::LengthOverflow)?;
                debug_assert_eq!(delta, 4);
                self.enc
                    .buf
                    .copy_within(self.payload_start..end, new_payload_start);
                self.enc.buf.truncate(end - delta);
                Ok(())
            }
            LenWidth::W8 => {
                self.enc.buf[self.tag_pos] = tags::TAG_MAP_LEN8;
                let count = u8::try_from(self.count).unwrap();
                let payload_len = u8::try_from(payload_len).unwrap();

                let count_pos = self.tag_pos + 1;
                self.enc.buf[count_pos] = count;
                self.enc.buf[count_pos + 1] = payload_len;

                let new_payload_start = self.tag_pos + 1 + 1 + 1;
                let end = self.enc.buf.len();
                let delta = self
                    .payload_start
                    .checked_sub(new_payload_start)
                    .ok_or(WireError::LengthOverflow)?;
                debug_assert_eq!(delta, 6);
                self.enc
                    .buf
                    .copy_within(self.payload_start..end, new_payload_start);
                self.enc.buf.truncate(end - delta);
                Ok(())
            }
        }
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    // A small, fast, dependency-free hash for duplicate-key detection.
    // Not intended for security; this is purely an encoder-side perf helper.
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

pub fn encode_value(value: &SerializedValueRef<'_>) -> Result<Vec<u8>> {
    let mut enc = Encoder::new();
    enc.value_ref(value)?;
    Ok(enc.into_inner())
}

/// Encodes `value` into `buf` using the default [`EncodeConfig`].
///
/// This function is intended for buffer reuse: it clears `buf` and then appends
/// the encoded bytes, keeping the existing capacity when possible.
pub fn encode_value_into(buf: &mut Vec<u8>, value: &SerializedValueRef<'_>) -> Result<()> {
    encode_value_into_with_config(buf, value, EncodeConfig::default())
}

/// Encodes `value` into `buf` using an explicit [`EncodeConfig`].
///
/// On success, `buf` contains exactly the encoded bytes.
/// On error, `buf` will contain whatever was written before the error, but its
/// capacity is preserved.
pub fn encode_value_into_with_config(
    buf: &mut Vec<u8>,
    value: &SerializedValueRef<'_>,
    config: EncodeConfig,
) -> Result<()> {
    buf.clear();

    // Avoid forcing the caller to move ownership of the buffer: we temporarily
    // move it into the encoder and then move it back out.
    let mut enc = Encoder::from_vec(std::mem::take(buf), config);
    let result = enc.value_ref(value);
    *buf = enc.into_inner();
    result
}
