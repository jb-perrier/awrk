use crate::codec::tags;
use crate::error::{Result, WireError};
use crate::value::{ArrayRef, MapRef, SerializedValueRef};

#[derive(Debug, Clone, Copy)]
pub struct DecodeConfig {
    pub max_depth: usize,
}

impl Default for DecodeConfig {
    fn default() -> Self {
        Self { max_depth: 64 }
    }
}

pub fn decode_value_full<'a>(
    buf: &'a [u8],
    config: DecodeConfig,
) -> Result<SerializedValueRef<'a>> {
    let (value, used) = decode_value(buf, config)?;
    if used != buf.len() {
        return Err(WireError::Malformed("trailing bytes after value"));
    }
    Ok(value)
}

pub fn decode_value<'a>(
    buf: &'a [u8],
    config: DecodeConfig,
) -> Result<(SerializedValueRef<'a>, usize)> {
    decode_value_at(buf, 0, config.max_depth)
}

fn decode_value_at<'a>(
    buf: &'a [u8],
    start: usize,
    depth: usize,
) -> Result<(SerializedValueRef<'a>, usize)> {
    if depth == 0 {
        return Err(WireError::RecursionLimitExceeded);
    }

    let tag = *buf.get(start).ok_or(WireError::UnexpectedEof)?;
    let mut cursor = start + 1;
    let value = match tag {
        tags::TAG_NULL => SerializedValueRef::Null,
        tags::TAG_UNIT => SerializedValueRef::Unit,
        tags::TAG_BOOL_FALSE => SerializedValueRef::Bool(false),
        tags::TAG_BOOL_TRUE => SerializedValueRef::Bool(true),
        tags::TAG_U64 => SerializedValueRef::U64(read_u64(buf, &mut cursor)?),
        tags::TAG_I64 => SerializedValueRef::I64(read_i64(buf, &mut cursor)?),
        tags::TAG_U8 => SerializedValueRef::U8(read_u8(buf, &mut cursor)?),
        tags::TAG_U16 => SerializedValueRef::U16(read_u16(buf, &mut cursor)?),
        tags::TAG_U32 => SerializedValueRef::U32(read_u32(buf, &mut cursor)?),
        tags::TAG_I8 => SerializedValueRef::I8(read_i8(buf, &mut cursor)?),
        tags::TAG_I16 => SerializedValueRef::I16(read_i16(buf, &mut cursor)?),
        tags::TAG_I32 => SerializedValueRef::I32(read_i32(buf, &mut cursor)?),
        tags::TAG_F32 => SerializedValueRef::F32(read_f32(buf, &mut cursor)?),
        tags::TAG_F64 => SerializedValueRef::F64(read_f64(buf, &mut cursor)?),
        tags::TAG_STRING | tags::TAG_STRING_LEN8 | tags::TAG_STRING_LEN16 => {
            let bytes = match tag {
                tags::TAG_STRING_LEN8 => read_len_prefixed_slice_u8(buf, &mut cursor)?,
                tags::TAG_STRING_LEN16 => read_len_prefixed_slice_u16(buf, &mut cursor)?,
                tags::TAG_STRING => read_len_prefixed_slice(buf, &mut cursor)?,
                _ => unreachable!(),
            };
            let value = core::str::from_utf8(bytes).map_err(|_| WireError::InvalidUtf8)?;
            SerializedValueRef::String(value)
        }
        tags::TAG_BYTES | tags::TAG_BYTES_LEN8 | tags::TAG_BYTES_LEN16 => {
            let bytes = match tag {
                tags::TAG_BYTES_LEN8 => read_len_prefixed_slice_u8(buf, &mut cursor)?,
                tags::TAG_BYTES_LEN16 => read_len_prefixed_slice_u16(buf, &mut cursor)?,
                tags::TAG_BYTES => read_len_prefixed_slice(buf, &mut cursor)?,
                _ => unreachable!(),
            };
            SerializedValueRef::Bytes(bytes)
        }
        tags::TAG_ARRAY | tags::TAG_ARRAY_LEN8 | tags::TAG_ARRAY_LEN16 => {
            let a = match tag {
                tags::TAG_ARRAY_LEN8 => decode_array_like_u8(buf, &mut cursor, depth - 1)?,
                tags::TAG_ARRAY_LEN16 => decode_array_like_u16(buf, &mut cursor, depth - 1)?,
                tags::TAG_ARRAY => decode_array_like(buf, &mut cursor, depth - 1)?,
                _ => unreachable!(),
            };
            SerializedValueRef::Array(a)
        }
        tags::TAG_MAP | tags::TAG_MAP_LEN8 | tags::TAG_MAP_LEN16 => {
            let m = match tag {
                tags::TAG_MAP_LEN8 => decode_map_u8(buf, &mut cursor, depth - 1)?,
                tags::TAG_MAP_LEN16 => decode_map_u16(buf, &mut cursor, depth - 1)?,
                tags::TAG_MAP => decode_map(buf, &mut cursor, depth - 1)?,
                _ => unreachable!(),
            };
            SerializedValueRef::Map(m)
        }
        _ => return Err(WireError::InvalidTag(tag)),
    };

    Ok((value, cursor - start))
}

fn decode_array_like<'a>(buf: &'a [u8], cursor: &mut usize, depth: usize) -> Result<ArrayRef<'a>> {
    let count = read_u32(buf, cursor)? as usize;

    let payload_len = read_u32(buf, cursor)? as usize;
    let payload_start = *cursor;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or(WireError::LengthOverflow)?;
    if payload_end > buf.len() {
        return Err(WireError::UnexpectedEof);
    }

    let payload = &buf[payload_start..payload_end];
    *cursor = payload_end;

    Ok(ArrayRef {
        payload,
        count,
        depth,
    })
}

fn decode_array_like_u16<'a>(
    buf: &'a [u8],
    cursor: &mut usize,
    depth: usize,
) -> Result<ArrayRef<'a>> {
    let count = read_u16(buf, cursor)? as usize;

    let payload_len = read_u16(buf, cursor)? as usize;
    let payload_start = *cursor;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or(WireError::LengthOverflow)?;
    if payload_end > buf.len() {
        return Err(WireError::UnexpectedEof);
    }

    let payload = &buf[payload_start..payload_end];
    *cursor = payload_end;

    Ok(ArrayRef {
        payload,
        count,
        depth,
    })
}

fn decode_array_like_u8<'a>(
    buf: &'a [u8],
    cursor: &mut usize,
    depth: usize,
) -> Result<ArrayRef<'a>> {
    let count = read_u8(buf, cursor)? as usize;

    let payload_len = read_u8(buf, cursor)? as usize;
    let payload_start = *cursor;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or(WireError::LengthOverflow)?;
    if payload_end > buf.len() {
        return Err(WireError::UnexpectedEof);
    }

    let payload = &buf[payload_start..payload_end];
    *cursor = payload_end;

    Ok(ArrayRef {
        payload,
        count,
        depth,
    })
}

fn decode_map<'a>(buf: &'a [u8], cursor: &mut usize, depth: usize) -> Result<MapRef<'a>> {
    let count = read_u32(buf, cursor)? as usize;

    let payload_len = read_u32(buf, cursor)? as usize;
    let payload_start = *cursor;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or(WireError::LengthOverflow)?;
    if payload_end > buf.len() {
        return Err(WireError::UnexpectedEof);
    }
    let payload = &buf[payload_start..payload_end];
    *cursor = payload_end;

    Ok(MapRef {
        payload,
        count,
        depth,
    })
}

fn decode_map_u16<'a>(buf: &'a [u8], cursor: &mut usize, depth: usize) -> Result<MapRef<'a>> {
    let count = read_u16(buf, cursor)? as usize;

    let payload_len = read_u16(buf, cursor)? as usize;
    let payload_start = *cursor;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or(WireError::LengthOverflow)?;
    if payload_end > buf.len() {
        return Err(WireError::UnexpectedEof);
    }
    let payload = &buf[payload_start..payload_end];
    *cursor = payload_end;

    Ok(MapRef {
        payload,
        count,
        depth,
    })
}

fn decode_map_u8<'a>(buf: &'a [u8], cursor: &mut usize, depth: usize) -> Result<MapRef<'a>> {
    let count = read_u8(buf, cursor)? as usize;

    let payload_len = read_u8(buf, cursor)? as usize;
    let payload_start = *cursor;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or(WireError::LengthOverflow)?;
    if payload_end > buf.len() {
        return Err(WireError::UnexpectedEof);
    }
    let payload = &buf[payload_start..payload_end];
    *cursor = payload_end;

    Ok(MapRef {
        payload,
        count,
        depth,
    })
}

fn read_u32(buf: &[u8], cursor: &mut usize) -> Result<u32> {
    let end = cursor.checked_add(4).ok_or(WireError::LengthOverflow)?;
    let bytes = buf.get(*cursor..end).ok_or(WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
}

fn read_u16(buf: &[u8], cursor: &mut usize) -> Result<u16> {
    let end = cursor.checked_add(2).ok_or(WireError::LengthOverflow)?;
    let bytes = buf.get(*cursor..end).ok_or(WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(u16::from_be_bytes(bytes.try_into().unwrap()))
}

fn read_u8(buf: &[u8], cursor: &mut usize) -> Result<u8> {
    let value = *buf.get(*cursor).ok_or(WireError::UnexpectedEof)?;
    *cursor += 1;
    Ok(value)
}

fn read_u64(buf: &[u8], cursor: &mut usize) -> Result<u64> {
    let end = cursor.checked_add(8).ok_or(WireError::LengthOverflow)?;
    let bytes = buf.get(*cursor..end).ok_or(WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(u64::from_be_bytes(bytes.try_into().unwrap()))
}

fn read_i64(buf: &[u8], cursor: &mut usize) -> Result<i64> {
    Ok(read_u64(buf, cursor)? as i64)
}

fn read_i32(buf: &[u8], cursor: &mut usize) -> Result<i32> {
    let end = cursor.checked_add(4).ok_or(WireError::LengthOverflow)?;
    let bytes = buf.get(*cursor..end).ok_or(WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(i32::from_be_bytes(bytes.try_into().unwrap()))
}

fn read_i16(buf: &[u8], cursor: &mut usize) -> Result<i16> {
    let end = cursor.checked_add(2).ok_or(WireError::LengthOverflow)?;
    let bytes = buf.get(*cursor..end).ok_or(WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(i16::from_be_bytes(bytes.try_into().unwrap()))
}

fn read_i8(buf: &[u8], cursor: &mut usize) -> Result<i8> {
    let value = read_u8(buf, cursor)?;
    Ok(i8::from_be_bytes([value]))
}

fn read_f32(buf: &[u8], cursor: &mut usize) -> Result<f32> {
    let end = cursor.checked_add(4).ok_or(WireError::LengthOverflow)?;
    let bytes = buf.get(*cursor..end).ok_or(WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(f32::from_be_bytes(bytes.try_into().unwrap()))
}

fn read_f64(buf: &[u8], cursor: &mut usize) -> Result<f64> {
    let end = cursor.checked_add(8).ok_or(WireError::LengthOverflow)?;
    let bytes = buf.get(*cursor..end).ok_or(WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(f64::from_be_bytes(bytes.try_into().unwrap()))
}

fn read_len_prefixed_slice<'a>(buf: &'a [u8], cursor: &mut usize) -> Result<&'a [u8]> {
    let len = read_u32(buf, cursor)? as usize;
    let end = cursor.checked_add(len).ok_or(WireError::LengthOverflow)?;
    let data = buf.get(*cursor..end).ok_or(WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(data)
}

fn read_len_prefixed_slice_u16<'a>(buf: &'a [u8], cursor: &mut usize) -> Result<&'a [u8]> {
    let len = read_u16(buf, cursor)? as usize;
    let end = cursor.checked_add(len).ok_or(WireError::LengthOverflow)?;
    let data = buf.get(*cursor..end).ok_or(WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(data)
}

fn read_len_prefixed_slice_u8<'a>(buf: &'a [u8], cursor: &mut usize) -> Result<&'a [u8]> {
    let len = read_u8(buf, cursor)? as usize;
    let end = cursor.checked_add(len).ok_or(WireError::LengthOverflow)?;
    let data = buf.get(*cursor..end).ok_or(WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(data)
}
