use std::io::{self, Read};

use base64::Engine as _;

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();

    let exfmt_bytes = parse_hex_string(&input).unwrap_or_else(|e| {
        eprintln!("failed to parse hex from stdin: {e}");
        std::process::exit(2);
    });

    let value = awrk_datex::codec::decode::decode_value_full(
        &exfmt_bytes,
        awrk_datex::codec::decode::DecodeConfig::default(),
    )
    .unwrap_or_else(|e| {
        eprintln!("failed to decode exfmt value: {e}");
        std::process::exit(2);
    });

    let json_value = to_json_value(value).unwrap_or_else(|e| {
        eprintln!("failed to convert to json: {e}");
        std::process::exit(2);
    });

    let json = serde_json::to_string(&json_value).unwrap();

    let exfmt_len = exfmt_bytes.len();
    let json_len = json.as_bytes().len();

    // Print JSON to stdout (as requested).
    println!("{json}");

    // Print size comparison to stderr so piping the JSON stays convenient.
    eprintln!("exfmt bytes: {exfmt_len}");
    eprintln!("json  bytes: {json_len}");
    if exfmt_len != 0 {
        let ratio = json_len as f64 / exfmt_len as f64;
        eprintln!("ratio (json/exfmt): {ratio:.3}x");
    }
}

fn to_json_value(
    v: awrk_datex::value::SerializedValueRef<'_>,
) -> Result<serde_json::Value, String> {
    use awrk_datex::value::SerializedValueRef as V;

    Ok(match v {
        V::Null => serde_json::Value::Null,
        // Unit isn't a native JSON type; keep it distinguishable from Null.
        V::Unit => serde_json::json!({"$unit": true}),
        V::Bool(b) => serde_json::Value::Bool(b),

        V::U8(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        V::U16(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        V::U32(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        V::U64(n) => serde_json::Value::Number(serde_json::Number::from(n)),

        V::I8(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        V::I16(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        V::I32(n) => serde_json::Value::Number(serde_json::Number::from(n)),
        V::I64(n) => serde_json::Value::Number(serde_json::Number::from(n)),

        V::F32(x) => serde_json::Number::from_f64(x as f64)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| serde_json::Value::String(x.to_string())),
        V::F64(x) => serde_json::Number::from_f64(x)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| serde_json::Value::String(x.to_string())),

        V::String(s) => serde_json::Value::String(s.to_string()),

        // JSON has no bytes type: represent as base64 (no padding) string.
        V::Bytes(b) => {
            let encoded = base64::engine::general_purpose::STANDARD_NO_PAD.encode(b);
            serde_json::Value::String(format!("b64:{encoded}"))
        }

        V::Array(a) => {
            let mut out = Vec::with_capacity(a.len());
            let mut it = a.iter();
            while let Some(entry) = it.next() {
                let entry = entry.map_err(|e| e.to_string())?;
                out.push(to_json_value(entry)?);
            }
            it.finish().map_err(|e| e.to_string())?;
            serde_json::Value::Array(out)
        }

        // JSON objects require string keys; exfmt maps allow arbitrary keys.
        // Represent as an array of [key,value] pairs.
        V::Map(m) => {
            let mut out: Vec<serde_json::Value> = Vec::with_capacity(m.len());
            let mut it = m.iter_pairs();
            while let Some(entry) = it.next() {
                let (k, val) = entry.map_err(|e| e.to_string())?;
                out.push(serde_json::Value::Array(vec![
                    to_json_value(k)?,
                    to_json_value(val)?,
                ]));
            }
            it.finish().map_err(|e| e.to_string())?;
            serde_json::Value::Array(out)
        }
    })
}

fn parse_hex_string(src: &str) -> Result<Vec<u8>, String> {
    fn hex_val(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }

    let bytes = src.as_bytes();
    let mut out = Vec::new();

    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() || b == b'_' {
            i += 1;
            continue;
        }

        let hi = hex_val(b).ok_or_else(|| format!("invalid hex at byte {i}"))?;
        i += 1;

        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'_') {
            i += 1;
        }
        if i >= bytes.len() {
            return Err("odd number of hex digits".into());
        }

        let lo = hex_val(bytes[i]).ok_or_else(|| format!("invalid hex at byte {i}"))?;
        i += 1;

        out.push((hi << 4) | lo);
    }

    Ok(out)
}
