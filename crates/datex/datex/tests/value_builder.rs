use awrk_datex::builder::Value;
use awrk_datex::codec::decode::{DecodeConfig, decode_value_full};
use awrk_datex::value::SerializedValueRef;

#[test]
fn owned_value_builder_encodes_and_decodes() {
    let v = Value::Array(vec![
        1u64.into(),
        Value::String("hi".to_string()),
        Value::Map(vec![
            (1u64.into(), true.into()),
            (2u64.into(), Value::Bytes(vec![9, 8, 7])),
        ]),
    ]);

    let bytes = v.to_bytes().expect("encode");
    let decoded = decode_value_full(&bytes, DecodeConfig::default()).expect("decode");

    let a = decoded.as_array().expect("array");
    assert_eq!(a.len(), 3);
    let mut it = a.iter();
    assert_eq!(
        it.next().transpose().unwrap(),
        Some(SerializedValueRef::U8(1))
    );
    assert_eq!(
        it.next().transpose().unwrap(),
        Some(SerializedValueRef::String("hi"))
    );
    let m = it
        .next()
        .transpose()
        .unwrap()
        .expect("map value")
        .as_map()
        .expect("map");
    assert!(it.next().is_none());
    it.finish().unwrap();

    let find = |key: u64| -> Option<SerializedValueRef<'_>> {
        let mut it = m.iter_pairs();
        let mut found = None;
        while let Some(entry) = it.next() {
            let (k, v) = entry.ok()?;
            if k.as_u64() == Some(key) {
                found = Some(v);
            }
        }
        it.finish().ok()?;
        found
    };

    assert_eq!(find(1), Some(SerializedValueRef::Bool(true)));
    assert_eq!(find(2), Some(SerializedValueRef::Bytes(&[9, 8, 7])));
}
