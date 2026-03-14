use awrk_datex::WireError;

#[test]
fn wire_error_display_messages_are_stable() {
    assert_eq!(
        WireError::UnexpectedEof.to_string(),
        "unexpected end of frame"
    );
    assert_eq!(
        WireError::InvalidTag(0x12).to_string(),
        "invalid wire tag: 0x12"
    );
    assert_eq!(
        WireError::InvalidUtf8.to_string(),
        "invalid utf-8 string data"
    );
    assert_eq!(
        WireError::LengthOverflow.to_string(),
        "length overflows frame bounds"
    );
    assert_eq!(
        WireError::RecursionLimitExceeded.to_string(),
        "value nesting limit exceeded"
    );
    assert_eq!(WireError::Malformed("x").to_string(), "malformed frame: x");
}
