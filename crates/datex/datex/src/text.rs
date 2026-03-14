use crate::value::Value;

/// Parse a human-editable `awrk_datex::value::Value` literal.
///
/// This is intentionally a small, dependency-free format used by tools (CLI/UI)
/// and is *not* part of the network protocol.
///
/// Supported (examples):
/// - `null`, `()`, `true`, `false`
/// - numbers: `123`, `-5`, `1.25`, `1e-3`, with optional suffixes like `42u8`, `-1i64`, `3.0f32`
/// - strings: `"hello"` (with basic escapes)
/// - bytes: `hex("deadbeef")`
/// - arrays: `[1, 2, 3]`
/// - maps: `{ "k": 1, 2: "v" }`
pub fn parse_value(input: &str) -> Result<Value, String> {
    let mut parser = Parser::new(input);
    let value = parser.parse_value()?;
    parser.skip_ws();
    if !parser.is_eof() {
        return Err(format!(
            "unexpected trailing input at byte {}",
            parser.cursor
        ));
    }
    Ok(value)
}

/// A compact, single-line representation.
pub fn format_value_compact(value: &Value) -> String {
    let mut out = String::new();
    fmt_compact(value, &mut out);
    out
}

/// A multi-line representation with indentation.
pub fn format_value_pretty(value: &Value) -> String {
    let mut out = String::new();
    fmt_pretty(value, 0, &mut out);
    out
}

fn fmt_compact(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Unit => out.push_str("()"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),

        Value::U8(v) => out.push_str(&format!("{v}u8")),
        Value::U16(v) => out.push_str(&format!("{v}u16")),
        Value::U32(v) => out.push_str(&format!("{v}u32")),
        Value::U64(v) => out.push_str(&v.to_string()),

        Value::I8(v) => out.push_str(&format!("{v}i8")),
        Value::I16(v) => out.push_str(&format!("{v}i16")),
        Value::I32(v) => out.push_str(&format!("{v}i32")),
        Value::I64(v) => out.push_str(&v.to_string()),

        Value::F32(v) => {
            out.push_str(&trim_float((*v as f64).to_string()));
            out.push_str("f32");
        }
        Value::F64(v) => out.push_str(&trim_float(v.to_string())),

        Value::String(s) => fmt_string(s, out),
        Value::Bytes(b) => {
            out.push_str("hex(\"");
            for byte in b {
                out.push_str(&format!("{byte:02x}"));
            }
            out.push_str("\")");
        }

        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i != 0 {
                    out.push_str(", ");
                }
                fmt_compact(item, out);
            }
            out.push(']');
        }
        Value::Map(entries) => {
            out.push('{');
            for (i, (k, v)) in entries.iter().enumerate() {
                if i != 0 {
                    out.push_str(", ");
                }
                fmt_compact(k, out);
                out.push_str(": ");
                fmt_compact(v, out);
            }
            out.push('}');
        }
    }
}

fn fmt_pretty(value: &Value, indent: usize, out: &mut String) {
    match value {
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            out.push('\n');
            for (i, item) in items.iter().enumerate() {
                if i != 0 {
                    out.push_str(",\n");
                }
                push_indent(indent + 2, out);
                fmt_pretty(item, indent + 2, out);
            }
            out.push('\n');
            push_indent(indent, out);
            out.push(']');
        }
        Value::Map(entries) => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            out.push('\n');
            for (i, (k, v)) in entries.iter().enumerate() {
                if i != 0 {
                    out.push_str(",\n");
                }
                push_indent(indent + 2, out);
                fmt_compact(k, out);
                out.push_str(": ");
                fmt_pretty(v, indent + 2, out);
            }
            out.push('\n');
            push_indent(indent, out);
            out.push('}');
        }
        _ => fmt_compact(value, out),
    }
}

fn push_indent(n: usize, out: &mut String) {
    for _ in 0..n {
        out.push(' ');
    }
}

fn fmt_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
}

fn trim_float(mut s: String) -> String {
    if let Some(dot) = s.find('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            // keep at least one digit after '.' to avoid "1." looking odd
            s.push('0');
        }
        // make sure we didn't remove everything after '.'
        if s.len() == dot + 1 {
            s.push('0');
        }
    }
    s
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokKind {
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Comma,
    Colon,
    String,
    Ident,
    Number,
    Eof,
}

#[derive(Clone, Debug)]
struct Tok {
    kind: TokKind,
    start: usize,
    end: usize,
}

struct Lexer<'a> {
    src: &'a str,
    cursor: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, cursor: 0 }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.src.as_bytes().get(self.cursor).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek_byte()?;
        self.cursor += 1;
        Some(b)
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek_byte() {
            if b.is_ascii_whitespace() {
                self.cursor += 1;
            } else {
                break;
            }
        }
    }

    fn next_tok(&mut self) -> Result<Tok, String> {
        self.skip_ws();
        let start = self.cursor;
        let Some(b) = self.peek_byte() else {
            return Ok(Tok {
                kind: TokKind::Eof,
                start,
                end: start,
            });
        };

        let kind = match b {
            b'{' => {
                self.cursor += 1;
                TokKind::LBrace
            }
            b'}' => {
                self.cursor += 1;
                TokKind::RBrace
            }
            b'[' => {
                self.cursor += 1;
                TokKind::LBracket
            }
            b']' => {
                self.cursor += 1;
                TokKind::RBracket
            }
            b'(' => {
                self.cursor += 1;
                TokKind::LParen
            }
            b')' => {
                self.cursor += 1;
                TokKind::RParen
            }
            b',' => {
                self.cursor += 1;
                TokKind::Comma
            }
            b':' => {
                self.cursor += 1;
                TokKind::Colon
            }
            b'"' => {
                self.scan_string()?;
                TokKind::String
            }
            b'-' | b'0'..=b'9' => {
                self.scan_number();
                TokKind::Number
            }
            _ => {
                if is_ident_start(b) {
                    self.scan_ident();
                    TokKind::Ident
                } else {
                    return Err(format!("unexpected byte {b:#x} at {start}"));
                }
            }
        };

        Ok(Tok {
            kind,
            start,
            end: self.cursor,
        })
    }

    fn scan_ident(&mut self) {
        self.cursor += 1;
        while let Some(b) = self.peek_byte() {
            if is_ident_continue(b) {
                self.cursor += 1;
            } else {
                break;
            }
        }
    }

    fn scan_number(&mut self) {
        // [+-]? DIGITS ('.' DIGITS)? ([eE] [+-]? DIGITS)? SUFFIX?
        if self.peek_byte() == Some(b'-') {
            self.cursor += 1;
        }
        while matches!(self.peek_byte(), Some(b'0'..=b'9')) {
            self.cursor += 1;
        }
        if self.peek_byte() == Some(b'.') {
            self.cursor += 1;
            while matches!(self.peek_byte(), Some(b'0'..=b'9')) {
                self.cursor += 1;
            }
        }
        if matches!(self.peek_byte(), Some(b'e') | Some(b'E')) {
            self.cursor += 1;
            if matches!(self.peek_byte(), Some(b'+') | Some(b'-')) {
                self.cursor += 1;
            }
            while matches!(self.peek_byte(), Some(b'0'..=b'9')) {
                self.cursor += 1;
            }
        }
        while matches!(self.peek_byte(), Some(b'a'..=b'z') | Some(b'0'..=b'9')) {
            self.cursor += 1;
        }
    }

    fn scan_string(&mut self) -> Result<(), String> {
        // consume opening quote
        let Some(b'"') = self.bump() else {
            return Err("expected '".to_string());
        };
        while let Some(b) = self.bump() {
            match b {
                b'"' => return Ok(()),
                b'\\' => {
                    // escape
                    let Some(esc) = self.bump() else {
                        return Err("unterminated string escape".to_string());
                    };
                    match esc {
                        b'"' | b'\\' | b'n' | b'r' | b't' => {}
                        _ => return Err(format!("unsupported string escape: \\{esc}")),
                    }
                }
                _ => {}
            }
        }
        Err("unterminated string".to_string())
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit() || b == b'-'
}

struct Parser<'a> {
    src: &'a str,
    lexer: Lexer<'a>,
    lookahead: Tok,
    cursor: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        let mut lexer = Lexer::new(src);
        let lookahead = lexer.next_tok().unwrap_or(Tok {
            kind: TokKind::Eof,
            start: 0,
            end: 0,
        });
        let cursor = lookahead.start;
        Self {
            src,
            lexer,
            lookahead,
            cursor,
        }
    }

    fn is_eof(&self) -> bool {
        self.lookahead.kind == TokKind::Eof
    }

    fn skip_ws(&mut self) {
        // lexer already skips ws on next_tok; keep cursor in sync for error messages
        self.cursor = self.lookahead.start;
    }

    fn bump(&mut self) -> Result<Tok, String> {
        let current = self.lookahead.clone();
        self.lookahead = self.lexer.next_tok()?;
        self.cursor = self.lookahead.start;
        Ok(current)
    }

    fn expect(&mut self, kind: TokKind) -> Result<Tok, String> {
        if self.lookahead.kind != kind {
            return Err(format!(
                "expected {kind:?} at byte {}, got {got:?}",
                self.lookahead.start,
                got = self.lookahead.kind
            ));
        }
        self.bump()
    }

    fn slice(&self, tok: &Tok) -> &'a str {
        &self.src[tok.start..tok.end]
    }

    fn parse_value(&mut self) -> Result<Value, String> {
        match self.lookahead.kind {
            TokKind::LParen => {
                self.bump()?;
                self.expect(TokKind::RParen)?;
                Ok(Value::Unit)
            }
            TokKind::LBracket => self.parse_array(),
            TokKind::LBrace => self.parse_map(),
            TokKind::String => {
                let tok = self.bump()?;
                let s = parse_string_literal(self.slice(&tok))?;
                Ok(Value::String(s))
            }
            TokKind::Ident => self.parse_ident_or_call(),
            TokKind::Number => {
                let tok = self.bump()?;
                parse_number_literal(self.slice(&tok))
            }
            _ => Err(format!(
                "unexpected token {k:?} at byte {}",
                self.lookahead.start,
                k = self.lookahead.kind
            )),
        }
    }

    fn parse_array(&mut self) -> Result<Value, String> {
        self.expect(TokKind::LBracket)?;
        let mut items = Vec::new();
        if self.lookahead.kind == TokKind::RBracket {
            self.bump()?;
            return Ok(Value::Array(items));
        }
        loop {
            let v = self.parse_value()?;
            items.push(v);
            match self.lookahead.kind {
                TokKind::Comma => {
                    self.bump()?;
                    if self.lookahead.kind == TokKind::RBracket {
                        self.bump()?;
                        break;
                    }
                }
                TokKind::RBracket => {
                    self.bump()?;
                    break;
                }
                _ => {
                    return Err(format!(
                        "expected ',' or ']' at byte {}",
                        self.lookahead.start
                    ));
                }
            }
        }
        Ok(Value::Array(items))
    }

    fn parse_map(&mut self) -> Result<Value, String> {
        self.expect(TokKind::LBrace)?;
        let mut entries = Vec::new();
        if self.lookahead.kind == TokKind::RBrace {
            self.bump()?;
            return Ok(Value::Map(entries));
        }
        loop {
            let k = self.parse_value()?;
            self.expect(TokKind::Colon)?;
            let v = self.parse_value()?;
            entries.push((k, v));
            match self.lookahead.kind {
                TokKind::Comma => {
                    self.bump()?;
                    if self.lookahead.kind == TokKind::RBrace {
                        self.bump()?;
                        break;
                    }
                }
                TokKind::RBrace => {
                    self.bump()?;
                    break;
                }
                _ => {
                    return Err(format!(
                        "expected ',' or '}}' at byte {}",
                        self.lookahead.start
                    ));
                }
            }
        }
        Ok(Value::Map(entries))
    }

    fn parse_ident_or_call(&mut self) -> Result<Value, String> {
        let tok = self.bump()?;
        let ident = self.slice(&tok);
        match ident {
            "null" => Ok(Value::Null),
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            "hex" => {
                self.expect(TokKind::LParen)?;
                let s_tok = self.expect(TokKind::String)?;
                let s = parse_string_literal(self.slice(&s_tok))?;
                self.expect(TokKind::RParen)?;
                let bytes = parse_hex_bytes(&s)?;
                Ok(Value::Bytes(bytes))
            }
            other => Err(format!("unknown identifier '{other}'")),
        }
    }
}

fn parse_string_literal(src: &str) -> Result<String, String> {
    let bytes = src.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'"' || bytes[bytes.len() - 1] != b'"' {
        return Err("invalid string literal".to_string());
    }
    let mut out = String::new();
    let mut i = 1;
    while i + 1 < bytes.len() {
        let b = bytes[i];
        if b == b'\\' {
            let esc = *bytes.get(i + 1).ok_or("unterminated escape")?;
            match esc {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                _ => return Err(format!("unsupported escape: \\{esc}")),
            }
            i += 2;
        } else {
            out.push(b as char);
            i += 1;
        }
    }
    Ok(out)
}

fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    let mut hex = String::new();
    for ch in s.chars() {
        if ch.is_ascii_hexdigit() {
            hex.push(ch);
        } else if ch.is_ascii_whitespace() || ch == '_' {
            continue;
        } else {
            return Err(format!("invalid hex character '{ch}'"));
        }
    }
    if hex.len() % 2 != 0 {
        return Err("hex string must have even length".to_string());
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = from_hex_digit(bytes[i] as char)?;
        let lo = from_hex_digit(bytes[i + 1] as char)?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn from_hex_digit(ch: char) -> Result<u8, String> {
    match ch {
        '0'..='9' => Ok((ch as u8) - b'0'),
        'a'..='f' => Ok((ch as u8) - b'a' + 10),
        'A'..='F' => Ok((ch as u8) - b'A' + 10),
        _ => Err(format!("invalid hex digit '{ch}'")),
    }
}

fn parse_number_literal(src: &str) -> Result<Value, String> {
    // Split suffix (like `u16`, `i64`, `f32`) from the numeric part.
    // We look for a trailing run of ASCII alphanumerics, and if it starts with
    // a letter, treat it as the suffix.
    let bytes = src.as_bytes();
    let mut run_start = src.len();
    while run_start > 0 {
        let b = bytes[run_start - 1];
        if b.is_ascii_alphanumeric() {
            run_start -= 1;
        } else {
            break;
        }
    }

    let (num_part, suffix) = if run_start < src.len() {
        let suffix_first = bytes[run_start];
        if (suffix_first as char).is_ascii_alphabetic() {
            src.split_at(run_start)
        } else {
            (src, "")
        }
    } else {
        (src, "")
    };
    let suffix = suffix;
    let is_float = num_part.contains('.') || num_part.contains('e') || num_part.contains('E');

    match suffix {
        "" => {
            if is_float {
                let f: f64 = num_part
                    .parse()
                    .map_err(|_| format!("invalid float: {src}"))?;
                return Ok(Value::F64(f));
            }
            if num_part.starts_with('-') {
                let i: i64 = num_part
                    .parse()
                    .map_err(|_| format!("invalid integer: {src}"))?;
                return Ok(Value::I64(i));
            }
            let u: u64 = num_part
                .parse()
                .map_err(|_| format!("invalid unsigned integer: {src}"))?;
            Ok(Value::U64(u))
        }

        "u8" => Ok(Value::U8(parse_int_range::<u8>(num_part, src)?)),
        "u16" => Ok(Value::U16(parse_int_range::<u16>(num_part, src)?)),
        "u32" => Ok(Value::U32(parse_int_range::<u32>(num_part, src)?)),
        "u64" => Ok(Value::U64(
            num_part
                .parse()
                .map_err(|_| format!("invalid u64: {src}"))?,
        )),

        "i8" => Ok(Value::I8(parse_int_range::<i8>(num_part, src)?)),
        "i16" => Ok(Value::I16(parse_int_range::<i16>(num_part, src)?)),
        "i32" => Ok(Value::I32(parse_int_range::<i32>(num_part, src)?)),
        "i64" => Ok(Value::I64(
            num_part
                .parse()
                .map_err(|_| format!("invalid i64: {src}"))?,
        )),

        "f32" => {
            let f: f32 = num_part
                .parse()
                .map_err(|_| format!("invalid f32: {src}"))?;
            Ok(Value::F32(f))
        }
        "f64" => {
            let f: f64 = num_part
                .parse()
                .map_err(|_| format!("invalid f64: {src}"))?;
            Ok(Value::F64(f))
        }

        _ => Err(format!("unknown numeric suffix '{suffix}'")),
    }
}

fn parse_int_range<T>(num_part: &str, full: &str) -> Result<T, String>
where
    T: core::str::FromStr,
{
    num_part
        .parse::<T>()
        .map_err(|_| format!("invalid integer: {full}"))
}
