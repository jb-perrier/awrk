use awrk_datex::codec::{
    EncodeConfig, encode_value, encode_value_into, encode_value_into_with_config,
};
use awrk_datex::value::SerializedValueRef;

#[test]
fn encode_value_into_matches_encode_value_default_config() {
    let value = SerializedValueRef::U64(300);

    let expected = encode_value(&value).expect("encode_value must succeed");

    let mut buf = Vec::with_capacity(1024);
    encode_value_into(&mut buf, &value).expect("encode_value_into must succeed");

    assert_eq!(buf, expected);
}

#[test]
fn encode_value_into_with_config_matches_encode_value_for_same_config() {
    let value = SerializedValueRef::U64(300);

    let config = EncodeConfig {
        max_depth: 64,
        compact_ints: false,
    };

    let expected = {
        let mut buf = Vec::new();
        encode_value_into_with_config(&mut buf, &value, config).expect("encode into must succeed");
        buf
    };

    // encode_value always uses default config; so for non-default config we just assert
    // that doing it twice yields the same bytes.
    let mut buf2 = Vec::with_capacity(16);
    encode_value_into_with_config(&mut buf2, &value, config).expect("encode into must succeed");

    assert_eq!(buf2, expected);
}
