use std::fs;
use std::io::{self, Read};

use awrk_datex as upi_wire;
use upi_wire::codec::tags as value_tags;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Default,
    Tag,
    Bool,
    Len,
    Payload,
    Utf8,
    Count,
    Error,
}

impl Style {
    fn ansi_prefix(self) -> &'static str {
        // 256-color backgrounds; keep text readable.
        // Note: Windows Terminal / modern conhost supports ANSI.
        match self {
            Style::Default => "\x1b[0m",
            Style::Tag => "\x1b[48;5;25m\x1b[38;5;231m", // blue bg, white fg
            Style::Bool => "\x1b[48;5;28m\x1b[38;5;231m", // green bg
            Style::Len => "\x1b[48;5;90m\x1b[38;5;231m", // dark magenta bg
            Style::Payload => "\x1b[48;5;238m\x1b[38;5;231m", // dark gray bg
            Style::Utf8 => "\x1b[48;5;220m\x1b[38;5;16m", // yellow bg, black fg
            Style::Count => "\x1b[48;5;67m\x1b[38;5;231m", // slate bg
            Style::Error => "\x1b[48;5;196m\x1b[38;5;231m", // bright red bg
        }
    }
}

#[derive(Debug, Clone)]
struct Span {
    start: usize,
    end: usize,
    style: Style,
    label: Option<String>,
}

fn main() {
    let mut args = std::env::args().skip(1);

    let mut tree = false;
    let mut input: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return;
            }
            "--tree" => {
                tree = true;
            }
            _ if input.is_none() => input = Some(arg),
            other => {
                eprintln!("unexpected argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let bytes = match input {
        Some(s) => read_input(&s).unwrap_or_else(|e| {
            eprintln!("failed to read input: {e}");
            std::process::exit(1);
        }),
        None => {
            // If no argument, read stdin as hex.
            let mut s = String::new();
            io::stdin().read_to_string(&mut s).unwrap();
            parse_hex_string(&s).unwrap_or_else(|e| {
                eprintln!("failed to parse stdin as hex: {e}");
                std::process::exit(1);
            })
        }
    };

    if bytes.is_empty() {
        eprintln!("empty payload");
        std::process::exit(2);
    }

    if tree {
        println!("bytes: {}", bytes.len());
        if let Err(e) = print_tree(&bytes) {
            eprintln!("decode error: {e}");
        }
        return;
    }

    let mut spans: Vec<Span> = Vec::new();
    let mut errors: Vec<(usize, usize)> = Vec::new();

    let parse_result = annotate_value(&bytes, 0, bytes.len(), 64, &mut spans, &mut errors);

    if let Err(msg) = parse_result {
        // If parsing failed, still render bytes; mark whole buffer as error.
        errors.push((0, bytes.len()));
        eprintln!("parse error: {msg}");
    }

    let mut style_map = vec![Style::Default; bytes.len()];
    for span in &spans {
        paint(&mut style_map, span.start, span.end, span.style);
    }
    for &(s, e) in &errors {
        paint(&mut style_map, s, e, Style::Error);
        spans.push(Span {
            start: s,
            end: e,
            style: Style::Error,
            label: None,
        });
    }

    println!("bytes: {}", bytes.len());
    print_hexdump(&bytes, &style_map, &spans);
}

fn print_help() {
    println!(
        "upi-wire-viz\n\nUSAGE:\n  upi-wire-viz [--tree] <HEX|@FILE|- >\n\nINPUT:\n  - HEX: hex bytes, spaces/underscores allowed (example: \"14 7b\")\n  - @FILE: read raw bytes from file (example: @payload.bin)\n  - If no input is provided, reads stdin as hex\n\nOUTPUT:\n  Default: colored hexdump of an encoded upi-wire Value.\n  With --tree: prints an indented Value tree (no hex dump).\n"
    );
}

fn print_tree(buf: &[u8]) -> Result<(), String> {
    let v = upi_wire::codec::decode::decode_value_full(
        buf,
        upi_wire::codec::decode::DecodeConfig { max_depth: 64 },
    )
    .map_err(|e| e.to_string())?;
    print_value_tree(&v, 0);
    Ok(())
}

fn print_value_tree(value: &upi_wire::value::SerializedValueRef<'_>, indent: usize) {
    let pad = "  ".repeat(indent);
    match value {
        upi_wire::value::SerializedValueRef::Null => println!("{pad}Null"),
        upi_wire::value::SerializedValueRef::Unit => println!("{pad}Unit"),
        upi_wire::value::SerializedValueRef::Bool(v) => println!("{pad}Bool {v}"),
        upi_wire::value::SerializedValueRef::U8(v) => println!("{pad}U8 {v}"),
        upi_wire::value::SerializedValueRef::U16(v) => println!("{pad}U16 {v}"),
        upi_wire::value::SerializedValueRef::U32(v) => println!("{pad}U32 {v}"),
        upi_wire::value::SerializedValueRef::U64(v) => println!("{pad}U64 {v}"),
        upi_wire::value::SerializedValueRef::I8(v) => println!("{pad}I8 {v}"),
        upi_wire::value::SerializedValueRef::I16(v) => println!("{pad}I16 {v}"),
        upi_wire::value::SerializedValueRef::I32(v) => println!("{pad}I32 {v}"),
        upi_wire::value::SerializedValueRef::I64(v) => println!("{pad}I64 {v}"),
        upi_wire::value::SerializedValueRef::F32(v) => println!("{pad}F32 {v}"),
        upi_wire::value::SerializedValueRef::F64(v) => println!("{pad}F64 {v}"),
        upi_wire::value::SerializedValueRef::String(s) => {
            println!("{pad}String {}", preview_utf8(s.as_bytes()))
        }
        upi_wire::value::SerializedValueRef::Bytes(b) => {
            println!("{pad}Bytes len={} {}", b.len(), preview_bytes_hex(b));
        }
        upi_wire::value::SerializedValueRef::Array(a) => {
            println!("{pad}Array len={}", a.len());
            let mut ok = true;
            let mut it = a.iter();
            for (i, entry) in it.by_ref().enumerate() {
                print!("{}  [{i}] ", pad);
                match entry {
                    Ok(v) => {
                        // If it's scalar, keep it on one line; otherwise recurse.
                        if matches!(
                            v,
                            upi_wire::value::SerializedValueRef::Null
                                | upi_wire::value::SerializedValueRef::Unit
                                | upi_wire::value::SerializedValueRef::Bool(_)
                                | upi_wire::value::SerializedValueRef::U8(_)
                                | upi_wire::value::SerializedValueRef::U16(_)
                                | upi_wire::value::SerializedValueRef::U32(_)
                                | upi_wire::value::SerializedValueRef::U64(_)
                                | upi_wire::value::SerializedValueRef::I8(_)
                                | upi_wire::value::SerializedValueRef::I16(_)
                                | upi_wire::value::SerializedValueRef::I32(_)
                                | upi_wire::value::SerializedValueRef::I64(_)
                                | upi_wire::value::SerializedValueRef::F32(_)
                                | upi_wire::value::SerializedValueRef::F64(_)
                                | upi_wire::value::SerializedValueRef::String(_)
                                | upi_wire::value::SerializedValueRef::Bytes(_)
                        ) {
                            // Inline scalar by calling with indent=0 into a temp-ish format.
                            // (We just print the type/value again; small duplication is OK.)
                            match v {
                                upi_wire::value::SerializedValueRef::Null => println!("Null"),
                                upi_wire::value::SerializedValueRef::Unit => println!("Unit"),
                                upi_wire::value::SerializedValueRef::Bool(v) => {
                                    println!("Bool {v}")
                                }
                                upi_wire::value::SerializedValueRef::U8(v) => println!("U8 {v}"),
                                upi_wire::value::SerializedValueRef::U16(v) => println!("U16 {v}"),
                                upi_wire::value::SerializedValueRef::U32(v) => println!("U32 {v}"),
                                upi_wire::value::SerializedValueRef::U64(v) => println!("U64 {v}"),
                                upi_wire::value::SerializedValueRef::I8(v) => println!("I8 {v}"),
                                upi_wire::value::SerializedValueRef::I16(v) => println!("I16 {v}"),
                                upi_wire::value::SerializedValueRef::I32(v) => println!("I32 {v}"),
                                upi_wire::value::SerializedValueRef::I64(v) => println!("I64 {v}"),
                                upi_wire::value::SerializedValueRef::F32(v) => println!("F32 {v}"),
                                upi_wire::value::SerializedValueRef::F64(v) => println!("F64 {v}"),
                                upi_wire::value::SerializedValueRef::String(s) => {
                                    println!("String {}", preview_utf8(s.as_bytes()))
                                }
                                upi_wire::value::SerializedValueRef::Bytes(b) => {
                                    println!("Bytes len={} {}", b.len(), preview_bytes_hex(b))
                                }
                                _ => println!("<unexpected>"),
                            }
                        } else {
                            println!();
                            print_value_tree(&v, indent + 2);
                        }
                    }
                    Err(e) => {
                        ok = false;
                        println!("<error: {}>", e);
                    }
                }
            }
            if ok {
                if let Err(e) = it.finish() {
                    println!("{pad}  <error: {}>", e);
                }
            }
        }
        upi_wire::value::SerializedValueRef::Map(m) => {
            println!("{pad}Map len={}", m.len());
            let mut ok = true;
            let mut it = m.iter_pairs();
            for (i, entry) in it.by_ref().enumerate() {
                match entry {
                    Ok((k, v)) => {
                        println!("{pad}  [{i}] key:");
                        print_value_tree(&k, indent + 4);
                        println!("{pad}  [{i}] value:");
                        print_value_tree(&v, indent + 4);
                    }
                    Err(e) => {
                        ok = false;
                        println!("{pad}  [{i}] <error: {}>", e);
                    }
                }
            }
            if ok {
                if let Err(e) = it.finish() {
                    println!("{pad}  <error: {}>", e);
                }
            }
        }
    }
}

// (schema/envelope visualization intentionally removed; upi-wire-viz renders Values only)

fn preview_bytes_hex(bytes: &[u8]) -> String {
    const MAX: usize = 16;
    let mut out = String::new();
    out.push('[');
    let shown = bytes.len().min(MAX);
    for i in 0..shown {
        if i != 0 {
            out.push(' ');
        }
        use core::fmt::Write as _;
        let _ = write!(&mut out, "{:02x}", bytes[i]);
    }
    if bytes.len() > MAX {
        out.push_str(" …");
    }
    out.push(']');
    out
}

fn read_input(arg: &str) -> Result<Vec<u8>, String> {
    if arg == "-" {
        let mut buf = Vec::new();
        io::stdin()
            .read_to_end(&mut buf)
            .map_err(|e| e.to_string())?;
        return Ok(buf);
    }

    if let Some(path) = arg.strip_prefix('@') {
        return fs::read(path).map_err(|e| e.to_string());
    }

    parse_hex_string(arg)
}

fn parse_hex_string(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut nybble: Option<u8> = None;

    for ch in s.chars() {
        let v = match ch {
            '0'..='9' => (ch as u8 - b'0') as u8,
            'a'..='f' => (ch as u8 - b'a' + 10) as u8,
            'A'..='F' => (ch as u8 - b'A' + 10) as u8,
            'x' | 'X' => continue, // allow 0x..
            ' ' | '\n' | '\r' | '\t' | '_' | '-' => continue,
            _ => return Err(format!("invalid hex char: {ch:?}")),
        };

        if let Some(hi) = nybble.take() {
            out.push((hi << 4) | v);
        } else {
            nybble = Some(v);
        }
    }

    if nybble.is_some() {
        return Err("odd number of hex digits".into());
    }

    Ok(out)
}

fn paint(style_map: &mut [Style], start: usize, end: usize, style: Style) {
    let end = end.min(style_map.len());
    let start = start.min(end);
    for s in &mut style_map[start..end] {
        *s = style;
    }
}

fn add_span(spans: &mut Vec<Span>, start: usize, end: usize, style: Style) {
    if start < end {
        spans.push(Span {
            start,
            end,
            style,
            label: None,
        });
    }
}

fn add_span_labeled<S: Into<String>>(
    spans: &mut Vec<Span>,
    start: usize,
    end: usize,
    style: Style,
    label: S,
) {
    if start < end {
        spans.push(Span {
            start,
            end,
            style,
            label: Some(label.into()),
        });
    }
}

fn annotate_value(
    buf: &[u8],
    start: usize,
    end: usize,
    depth: usize,
    spans: &mut Vec<Span>,
    errors: &mut Vec<(usize, usize)>,
) -> Result<(), String> {
    if depth == 0 {
        errors.push((start.min(end), end));
        return Err("recursion limit exceeded".into());
    }
    if start >= end {
        errors.push((start, end));
        return Err("unexpected eof".into());
    }

    let tag = buf[start];
    let tag_label = match tag {
        value_tags::TAG_NULL => "NULL".to_string(),
        value_tags::TAG_BOOL_FALSE => "BOOL_FALSE(false)".to_string(),
        value_tags::TAG_BOOL_TRUE => "BOOL_TRUE(true)".to_string(),
        value_tags::TAG_UNIT => "UNIT".to_string(),
        value_tags::TAG_U8 => "U8".to_string(),
        value_tags::TAG_U16 => "U16".to_string(),
        value_tags::TAG_U32 => "U32".to_string(),
        value_tags::TAG_U64 => "U64".to_string(),
        value_tags::TAG_I8 => "I8".to_string(),
        value_tags::TAG_I16 => "I16".to_string(),
        value_tags::TAG_I32 => "I32".to_string(),
        value_tags::TAG_I64 => "I64".to_string(),
        value_tags::TAG_F32 => "F32".to_string(),
        value_tags::TAG_F64 => "F64".to_string(),
        value_tags::TAG_STRING => "STRING_LEN32".to_string(),
        value_tags::TAG_STRING_LEN16 => "STRING_LEN16".to_string(),
        value_tags::TAG_STRING_LEN8 => "STRING_LEN8".to_string(),
        value_tags::TAG_BYTES => "BYTES_LEN32".to_string(),
        value_tags::TAG_BYTES_LEN16 => "BYTES_LEN16".to_string(),
        value_tags::TAG_BYTES_LEN8 => "BYTES_LEN8".to_string(),
        value_tags::TAG_ARRAY => "ARRAY_LEN32".to_string(),
        value_tags::TAG_ARRAY_LEN16 => "ARRAY_LEN16".to_string(),
        value_tags::TAG_ARRAY_LEN8 => "ARRAY_LEN8".to_string(),
        value_tags::TAG_MAP => "MAP_LEN32".to_string(),
        value_tags::TAG_MAP_LEN16 => "MAP_LEN16".to_string(),
        value_tags::TAG_MAP_LEN8 => "MAP_LEN8".to_string(),
        _ => format!("<invalid value tag {tag:#x}>").to_string(),
    };
    add_span_labeled(spans, start, start + 1, Style::Tag, tag_label);

    let mut cursor = start + 1;
    match tag {
        value_tags::TAG_NULL | value_tags::TAG_UNIT => {
            // No payload.
        }
        value_tags::TAG_BOOL_FALSE | value_tags::TAG_BOOL_TRUE => {
            add_span(spans, start, cursor, Style::Bool);
            if cursor != end {
                // If this is a sub-segment decode, let the caller validate exact length.
            }
        }
        value_tags::TAG_U8 | value_tags::TAG_I8 => {
            let pos = cursor;
            let raw = read_u8(buf, &mut cursor).map_err(|e| e.to_string())?;
            let label = if tag == value_tags::TAG_I8 {
                let v = i8::from_be_bytes([raw]);
                format!("{v}")
            } else {
                format!("{raw}")
            };
            add_span_labeled(spans, pos, pos + 1, Style::Utf8, label);
        }
        value_tags::TAG_U16 | value_tags::TAG_I16 => {
            let pos = cursor;
            let raw = read_u16(buf, &mut cursor).map_err(|e| e.to_string())?;
            let label = if tag == value_tags::TAG_I16 {
                let v = i16::from_be_bytes(raw.to_be_bytes());
                format!("{v}")
            } else {
                format!("{raw}")
            };
            add_span_labeled(spans, pos, pos + 2, Style::Utf8, label);
        }
        value_tags::TAG_U32 | value_tags::TAG_I32 => {
            let pos = cursor;
            let raw = read_u32(buf, &mut cursor).map_err(|e| e.to_string())?;
            let label = if tag == value_tags::TAG_I32 {
                let v = i32::from_be_bytes(raw.to_be_bytes());
                format!("{v}")
            } else {
                format!("{raw}")
            };
            add_span_labeled(spans, pos, pos + 4, Style::Utf8, label);
        }
        value_tags::TAG_U64 | value_tags::TAG_I64 => {
            let pos = cursor;
            let raw = read_u64(buf, &mut cursor).map_err(|e| e.to_string())?;
            let label = if tag == value_tags::TAG_I64 {
                let v = i64::from_be_bytes(raw.to_be_bytes());
                format!("{v}")
            } else {
                format!("{raw}")
            };
            add_span_labeled(spans, pos, pos + 8, Style::Utf8, label);
        }
        value_tags::TAG_F32 => {
            let pos = cursor;
            let raw = read_u32(buf, &mut cursor).map_err(|e| e.to_string())?;
            let v = f32::from_be_bytes(raw.to_be_bytes());
            add_span_labeled(spans, pos, pos + 4, Style::Utf8, format!("{v}"));
        }
        value_tags::TAG_F64 => {
            let pos = cursor;
            let raw = read_u64(buf, &mut cursor).map_err(|e| e.to_string())?;
            let v = f64::from_be_bytes(raw.to_be_bytes());
            add_span_labeled(spans, pos, pos + 8, Style::Utf8, format!("{v}"));
        }
        value_tags::TAG_STRING | value_tags::TAG_STRING_LEN16 | value_tags::TAG_STRING_LEN8 => {
            let len_pos = cursor;
            let (len, len_bytes) = match tag {
                value_tags::TAG_STRING_LEN8 => (
                    read_u8(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    1usize,
                ),
                value_tags::TAG_STRING_LEN16 => (
                    read_u16(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    2usize,
                ),
                value_tags::TAG_STRING => (
                    read_u32(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    4usize,
                ),
                _ => unreachable!(),
            };
            add_span_labeled(
                spans,
                len_pos,
                len_pos + len_bytes,
                Style::Len,
                format!("byte_len:{len}"),
            );
            let bytes_pos = cursor;
            let bytes_end = cursor.checked_add(len).ok_or("length overflow")?;
            if bytes_end > end {
                errors.push((bytes_pos, end));
                return Err("unexpected eof".into());
            }
            let preview = preview_utf8(&buf[bytes_pos..bytes_end]);
            add_span_labeled(spans, bytes_pos, bytes_end, Style::Utf8, preview);
            // UTF-8 validity check.
            if core::str::from_utf8(&buf[bytes_pos..bytes_end]).is_err() {
                errors.push((bytes_pos, bytes_end));
            }
            cursor = bytes_end;
        }
        value_tags::TAG_BYTES | value_tags::TAG_BYTES_LEN16 | value_tags::TAG_BYTES_LEN8 => {
            let len_pos = cursor;
            let (len, len_bytes) = match tag {
                value_tags::TAG_BYTES_LEN8 => (
                    read_u8(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    1usize,
                ),
                value_tags::TAG_BYTES_LEN16 => (
                    read_u16(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    2usize,
                ),
                value_tags::TAG_BYTES => (
                    read_u32(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    4usize,
                ),
                _ => unreachable!(),
            };
            add_span_labeled(
                spans,
                len_pos,
                len_pos + len_bytes,
                Style::Len,
                format!("byte_len:{len}"),
            );
            let bytes_pos = cursor;
            let bytes_end = cursor.checked_add(len).ok_or("length overflow")?;
            if bytes_end > end {
                errors.push((bytes_pos, end));
                return Err("unexpected eof".into());
            }
            add_span_labeled(spans, bytes_pos, bytes_end, Style::Utf8, "bytes");
            cursor = bytes_end;
        }
        value_tags::TAG_ARRAY | value_tags::TAG_ARRAY_LEN16 | value_tags::TAG_ARRAY_LEN8 => {
            let count_pos = cursor;
            let (count, count_bytes) = match tag {
                value_tags::TAG_ARRAY_LEN8 => (
                    read_u8(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    1usize,
                ),
                value_tags::TAG_ARRAY_LEN16 => (
                    read_u16(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    2usize,
                ),
                value_tags::TAG_ARRAY => (
                    read_u32(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    4usize,
                ),
                _ => unreachable!(),
            };
            add_span_labeled(
                spans,
                count_pos,
                count_pos + count_bytes,
                Style::Count,
                format!("count:{count}"),
            );

            let payload_len_pos = cursor;
            let (payload_len, len_bytes) = match tag {
                value_tags::TAG_ARRAY_LEN8 => (
                    read_u8(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    1usize,
                ),
                value_tags::TAG_ARRAY_LEN16 => (
                    read_u16(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    2usize,
                ),
                value_tags::TAG_ARRAY => (
                    read_u32(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    4usize,
                ),
                _ => unreachable!(),
            };
            add_span_labeled(
                spans,
                payload_len_pos,
                payload_len_pos + len_bytes,
                Style::Len,
                format!("payload_len:{payload_len}"),
            );

            let payload_pos = cursor;
            let payload_end = cursor.checked_add(payload_len).ok_or("length overflow")?;
            if payload_end > end {
                errors.push((payload_pos, end));
                return Err("unexpected eof".into());
            }
            add_span(spans, payload_pos, payload_end, Style::Payload);

            // Annotate nested values by decoding sequentially.
            let cfg = upi_wire::codec::decode::DecodeConfig {
                max_depth: depth - 1,
            };
            let mut at = payload_pos;
            for _ in 0..count {
                if at >= payload_end {
                    errors.push((at, payload_end));
                    break;
                }
                let segment = &buf[at..payload_end];
                if let Ok((_v, used)) = upi_wire::codec::decode::decode_value(segment, cfg) {
                    let end_val = at + used;
                    let _ = annotate_value(buf, at, end_val, depth - 1, spans, errors);
                    at = end_val;
                } else {
                    errors.push((at, payload_end));
                    break;
                }
            }
            if at != payload_end {
                errors.push((at, payload_end));
            }

            cursor = payload_end;
        }
        value_tags::TAG_MAP | value_tags::TAG_MAP_LEN16 | value_tags::TAG_MAP_LEN8 => {
            let count_pos = cursor;
            let (count, count_bytes) = match tag {
                value_tags::TAG_MAP_LEN8 => (
                    read_u8(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    1usize,
                ),
                value_tags::TAG_MAP_LEN16 => (
                    read_u16(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    2usize,
                ),
                value_tags::TAG_MAP => (
                    read_u32(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    4usize,
                ),
                _ => unreachable!(),
            };
            add_span_labeled(
                spans,
                count_pos,
                count_pos + count_bytes,
                Style::Count,
                format!("count:{count}"),
            );

            let payload_len_pos = cursor;
            let (payload_len, len_bytes) = match tag {
                value_tags::TAG_MAP_LEN8 => (
                    read_u8(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    1usize,
                ),
                value_tags::TAG_MAP_LEN16 => (
                    read_u16(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    2usize,
                ),
                value_tags::TAG_MAP => (
                    read_u32(buf, &mut cursor).map_err(|e| e.to_string())? as usize,
                    4usize,
                ),
                _ => unreachable!(),
            };
            add_span_labeled(
                spans,
                payload_len_pos,
                payload_len_pos + len_bytes,
                Style::Len,
                format!("payload_len:{payload_len}"),
            );

            let payload_pos = cursor;
            let payload_end = cursor.checked_add(payload_len).ok_or("length overflow")?;
            if payload_end > end {
                errors.push((payload_pos, end));
                return Err("unexpected eof".into());
            }
            add_span(spans, payload_pos, payload_end, Style::Payload);

            // Annotate entries by decoding sequentially.
            let cfg = upi_wire::codec::decode::DecodeConfig {
                max_depth: depth - 1,
            };
            let mut at = payload_pos;
            for _ in 0..count {
                if at >= payload_end {
                    errors.push((at, payload_end));
                    break;
                }

                let segment = &buf[at..payload_end];
                if let Ok((_k, used_key)) = upi_wire::codec::decode::decode_value(segment, cfg) {
                    let key_end = at + used_key;
                    let _ = annotate_value(buf, at, key_end, depth - 1, spans, errors);

                    let value_segment = buf.get(key_end..payload_end).unwrap_or(&[]);
                    if value_segment.is_empty() {
                        errors.push((key_end, payload_end));
                        break;
                    }
                    if let Ok((_v, used_val)) =
                        upi_wire::codec::decode::decode_value(value_segment, cfg)
                    {
                        let val_end = key_end + used_val;
                        let _ = annotate_value(buf, key_end, val_end, depth - 1, spans, errors);
                        at = val_end;
                    } else {
                        errors.push((key_end, payload_end));
                        break;
                    }
                } else {
                    errors.push((at, payload_end));
                    break;
                }
            }
            if at != payload_end {
                errors.push((at, payload_end));
            }

            cursor = payload_end;
        }
        _ => {
            errors.push((start, start + 1));
            return Err(format!("invalid value tag: {tag:#x}"));
        }
    }

    // If we're parsing the top-level (start==0 and end==buf.len()), require full consumption.
    if start == 0 && end == buf.len() && cursor != end {
        errors.push((cursor, end));
        return Err("trailing bytes after value".into());
    }

    Ok(())
}

// Legend printing intentionally removed to keep output compact.

fn print_hexdump(buf: &[u8], styles: &[Style], spans: &[Span]) {
    const WIDTH: usize = 16;

    // Compute the "owner" span per byte so we can color the separator spaces only
    // when adjacent bytes belong to the same semantic value (same span).
    let mut owner: Vec<usize> = vec![usize::MAX; buf.len()];
    let mut best_len: Vec<usize> = vec![usize::MAX; buf.len()];
    for (idx, span) in spans.iter().enumerate() {
        let start = span.start.min(buf.len());
        let end = span.end.min(buf.len());
        if start >= end {
            continue;
        }
        let len = end - start;
        for pos in start..end {
            if span.style == Style::Error {
                owner[pos] = idx;
                best_len[pos] = 0;
                continue;
            }
            if len < best_len[pos] {
                owner[pos] = idx;
                best_len[pos] = len;
            }
        }
    }

    // Map byte offsets to label indices. Only spans with labels are included.
    let mut labels_at: Vec<Vec<usize>> = vec![Vec::new(); buf.len() + 1];
    for (idx, span) in spans.iter().enumerate() {
        if span.label.is_some() && span.start <= buf.len() {
            labels_at[span.start].push(idx);
        }
    }

    for (line_idx, chunk) in buf.chunks(WIDTH).enumerate() {
        let base = line_idx * WIDTH;
        print!("{base:08x}  ");

        for i in 0..WIDTH {
            if i < chunk.len() {
                let b = chunk[i];
                let style = styles[base + i];
                print!("{}{:02x}", style.ansi_prefix(), b);

                // Color the space separator iff the next byte belongs to the same span.
                let abs = base + i;
                let same_owner = (i + 1) < chunk.len()
                    && owner[abs] != usize::MAX
                    && owner[abs] == owner[abs + 1];
                if same_owner {
                    print!(" \x1b[0m");
                } else {
                    print!("\x1b[0m ");
                }
            } else {
                print!("   ");
            }
        }

        print!(" ");
        let mut first = true;
        for i in 0..chunk.len() {
            let abs = base + i;
            for idx in &labels_at[abs] {
                if !first {
                    print!(" ");
                }
                first = false;
                let span = &spans[*idx];
                let label = span.label.as_deref().unwrap_or("");
                print!("{}{}\x1b[0m", span.style.ansi_prefix(), label);
            }
        }
        // Reset and clear-to-EOL to avoid background color extending past the
        // last printed character.
        print!("\x1b[0m\x1b[K\n");
    }
}

fn preview_utf8(bytes: &[u8]) -> String {
    const MAX_CHARS: usize = 32;
    let Ok(s) = core::str::from_utf8(bytes) else {
        return "<invalid utf8>".to_string();
    };

    let mut out = String::new();
    out.push('"');
    for ch in s.chars().take(MAX_CHARS) {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push('.'),
            c => out.push(c),
        }
    }
    if s.chars().count() > MAX_CHARS {
        out.push_str("…");
    }
    out.push('"');
    out
}

fn read_u8(buf: &[u8], cursor: &mut usize) -> upi_wire::Result<u8> {
    let b = *buf.get(*cursor).ok_or(upi_wire::WireError::UnexpectedEof)?;
    *cursor += 1;
    Ok(b)
}

fn read_u16(buf: &[u8], cursor: &mut usize) -> upi_wire::Result<u16> {
    let end = cursor
        .checked_add(2)
        .ok_or(upi_wire::WireError::LengthOverflow)?;
    let bytes = buf
        .get(*cursor..end)
        .ok_or(upi_wire::WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(u16::from_be_bytes(bytes.try_into().unwrap()))
}

fn read_u32(buf: &[u8], cursor: &mut usize) -> upi_wire::Result<u32> {
    let end = cursor
        .checked_add(4)
        .ok_or(upi_wire::WireError::LengthOverflow)?;
    let bytes = buf
        .get(*cursor..end)
        .ok_or(upi_wire::WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
}

fn read_u64(buf: &[u8], cursor: &mut usize) -> upi_wire::Result<u64> {
    let end = cursor
        .checked_add(8)
        .ok_or(upi_wire::WireError::LengthOverflow)?;
    let bytes = buf
        .get(*cursor..end)
        .ok_or(upi_wire::WireError::UnexpectedEof)?;
    *cursor = end;
    Ok(u64::from_be_bytes(bytes.try_into().unwrap()))
}
