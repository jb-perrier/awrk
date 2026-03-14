use awrk_datex::WireError;
use awrk_datex::codec::tags;
use awrk_datex::codec::{DecodeConfig, EncodeConfig, Encoder, decode_value, decode_value_full};
use awrk_datex::value::SerializedValueRef;

#[test]
fn decode_value_sequential_scalars_non_compacted() {
    let config = EncodeConfig {
        compact_ints: false,
        ..EncodeConfig::default()
    };

    let mut enc = Encoder::with_config(config);
    enc.bool(false);
    enc.bool(true);
    enc.u8(7);
    enc.u16(500);
    enc.u32(70_000);
    enc.u64(5_000_000_000);
    enc.i8(-7);
    enc.i16(-500);
    enc.i32(-70_000);
    enc.i64(-5_000_000_000);
    enc.f32(1.5);
    enc.f64(-2.25);
    enc.string("hello").unwrap();
    enc.bytes(&[1, 2, 3, 4]).unwrap();

    let buf = enc.into_inner();
    let mut cursor = 0usize;
    let decode_config = DecodeConfig::default();

    let expected: &[SerializedValueRef<'_>] = &[
        SerializedValueRef::Bool(false),
        SerializedValueRef::Bool(true),
        SerializedValueRef::U8(7),
        SerializedValueRef::U16(500),
        SerializedValueRef::U32(70_000),
        SerializedValueRef::U64(5_000_000_000),
        SerializedValueRef::I8(-7),
        SerializedValueRef::I16(-500),
        SerializedValueRef::I32(-70_000),
        SerializedValueRef::I64(-5_000_000_000),
        SerializedValueRef::F32(1.5),
        SerializedValueRef::F64(-2.25),
        SerializedValueRef::String("hello"),
        SerializedValueRef::Bytes(&[1, 2, 3, 4]),
    ];

    for exp in expected {
        let (v, used) = decode_value(&buf[cursor..], decode_config).unwrap();
        assert!(used > 0);
        assert_eq!(v, *exp);
        cursor += used;
    }

    assert_eq!(cursor, buf.len());
}

#[test]
fn decode_value_full_rejects_trailing_bytes() {
    let mut enc = Encoder::new();
    enc.bool(true);
    enc.bool(false);
    let buf = enc.into_inner();

    let err = decode_value_full(&buf, DecodeConfig::default()).unwrap_err();
    assert_eq!(err, WireError::Malformed("trailing bytes after value"));
}

#[test]
fn decode_array_and_map_roundtrip() {
    let config = EncodeConfig {
        compact_ints: false,
        ..EncodeConfig::default()
    };
    let mut enc = Encoder::with_config(config);

    // Exercise ARRAY/MAP decode and also cover the `SeqWriter::bytes` wrapper.
    enc.array(3, |w| {
        w.u32(123)?;
        w.bytes(&[9, 8, 7])?;
        w.string("abc")?;
        Ok(())
    })
    .unwrap();

    enc.map(2, |m| {
        m.entry(
            |e| e.string("k1"),
            |e| {
                e.i16(-12);
                Ok(())
            },
        )?;
        m.entry(
            |e| e.string("k2"),
            |e| {
                e.bytes(&[0xAA, 0xBB])?;
                Ok(())
            },
        )?;
        Ok(())
    })
    .unwrap();

    let buf = enc.into_inner();
    let decode_config = DecodeConfig::default();

    // Decode the array.
    let (v0, used0) = decode_value(&buf, decode_config).unwrap();
    let array = v0.as_array().unwrap();
    assert_eq!(array.len(), 3);
    let mut it = array.iter();
    assert_eq!(it.next().unwrap().unwrap(), SerializedValueRef::U32(123));
    assert_eq!(
        it.next().unwrap().unwrap(),
        SerializedValueRef::Bytes(&[9, 8, 7])
    );
    assert_eq!(
        it.next().unwrap().unwrap(),
        SerializedValueRef::String("abc")
    );
    assert!(it.next().is_none());
    it.finish().unwrap();

    // Decode the map right after it.
    let (v1, used1) = decode_value(&buf[used0..], decode_config).unwrap();
    let map = v1.as_map().unwrap();
    assert_eq!(map.len(), 2);
    let mut it = map.iter_pairs();
    assert_eq!(
        it.next().unwrap().unwrap(),
        (
            SerializedValueRef::String("k1"),
            SerializedValueRef::I16(-12)
        )
    );
    assert_eq!(
        it.next().unwrap().unwrap(),
        (
            SerializedValueRef::String("k2"),
            SerializedValueRef::Bytes(&[0xAA, 0xBB])
        )
    );
    assert!(it.next().is_none());
    it.finish().unwrap();

    assert_eq!(used0 + used1, buf.len());
}

#[test]
fn decode_invalid_container_payload_len_is_error() {
    // Minimal ARRAY header with payload_len exceeding the available buffer.
    let mut buf = Vec::new();
    buf.push(tags::TAG_ARRAY);
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(&1u32.to_be_bytes());

    let err = decode_value_full(&buf, DecodeConfig::default()).unwrap_err();
    assert_eq!(err, WireError::UnexpectedEof);
}

#[test]
fn decode_container_empty_element_is_error() {
    // ARRAY(count=1, payload_len=0, payload=[]).
    // The container header can be decoded, but lazy element decode fails.
    let mut buf = Vec::new();
    buf.push(tags::TAG_ARRAY);
    buf.extend_from_slice(&1u32.to_be_bytes());
    buf.extend_from_slice(&0u32.to_be_bytes());

    let v = decode_value_full(&buf, DecodeConfig::default()).unwrap();
    let a = v.as_array().unwrap();
    let mut it = a.iter();
    let err = it.next().unwrap().unwrap_err();
    assert_eq!(err, WireError::UnexpectedEof);
}
