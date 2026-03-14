use awrk_datex::value::Value;
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ValueKind {
    Null,
    Unit,
    Bool,

    U8,
    U16,
    U32,
    U64,

    I8,
    I16,
    I32,
    I64,

    F32,
    F64,

    String,
    Bytes,
    Array,
    Map,
}

impl ValueKind {
    fn label(self) -> &'static str {
        match self {
            ValueKind::Null => "Null",
            ValueKind::Unit => "Unit",
            ValueKind::Bool => "Bool",

            ValueKind::U8 => "U8",
            ValueKind::U16 => "U16",
            ValueKind::U32 => "U32",
            ValueKind::U64 => "U64",

            ValueKind::I8 => "I8",
            ValueKind::I16 => "I16",
            ValueKind::I32 => "I32",
            ValueKind::I64 => "I64",

            ValueKind::F32 => "F32",
            ValueKind::F64 => "F64",

            ValueKind::String => "String",
            ValueKind::Bytes => "Bytes",
            ValueKind::Array => "Array",
            ValueKind::Map => "Map",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ValueEditorState {
    pub(crate) kind: ValueKind,
    pub(crate) bool_value: bool,
    pub(crate) text: String,
    pub(crate) items: Vec<ValueEditorState>,
    pub(crate) entries: Vec<(ValueEditorState, ValueEditorState)>,
    pub(crate) error: String,
}

impl Default for ValueEditorState {
    fn default() -> Self {
        Self::null()
    }
}

impl ValueEditorState {
    pub(crate) fn null() -> Self {
        Self {
            kind: ValueKind::Null,
            bool_value: false,
            text: String::new(),
            items: Vec::new(),
            entries: Vec::new(),
            error: String::new(),
        }
    }

    pub(crate) fn from_value(v: &Value) -> Self {
        match v {
            Value::Null => Self::null(),
            Value::Unit => Self {
                kind: ValueKind::Unit,
                ..Self::null()
            },
            Value::Bool(b) => Self {
                kind: ValueKind::Bool,
                bool_value: *b,
                ..Self::null()
            },

            Value::U8(x) => Self::num(ValueKind::U8, x.to_string()),
            Value::U16(x) => Self::num(ValueKind::U16, x.to_string()),
            Value::U32(x) => Self::num(ValueKind::U32, x.to_string()),
            Value::U64(x) => Self::num(ValueKind::U64, x.to_string()),

            Value::I8(x) => Self::num(ValueKind::I8, x.to_string()),
            Value::I16(x) => Self::num(ValueKind::I16, x.to_string()),
            Value::I32(x) => Self::num(ValueKind::I32, x.to_string()),
            Value::I64(x) => Self::num(ValueKind::I64, x.to_string()),

            Value::F32(x) => Self::num(ValueKind::F32, x.to_string()),
            Value::F64(x) => Self::num(ValueKind::F64, x.to_string()),

            Value::String(s) => Self {
                kind: ValueKind::String,
                text: s.clone(),
                ..Self::null()
            },
            Value::Bytes(bytes) => {
                let mut hex = String::with_capacity(bytes.len() * 2);
                for b in bytes {
                    hex.push_str(&format!("{b:02x}"));
                }
                Self {
                    kind: ValueKind::Bytes,
                    text: hex,
                    ..Self::null()
                }
            }
            Value::Array(items) => Self {
                kind: ValueKind::Array,
                items: items.iter().map(Self::from_value).collect(),
                ..Self::null()
            },
            Value::Map(entries) => Self {
                kind: ValueKind::Map,
                entries: entries
                    .iter()
                    .map(|(k, v)| (Self::from_value(k), Self::from_value(v)))
                    .collect(),
                ..Self::null()
            },
        }
    }

    fn num(kind: ValueKind, text: String) -> Self {
        Self {
            kind,
            text,
            ..Self::null()
        }
    }

    pub(crate) fn try_build_value(&mut self) -> Result<Value, String> {
        self.error.clear();
        let res = match self.kind {
            ValueKind::Null => Ok(Value::Null),
            ValueKind::Unit => Ok(Value::Unit),
            ValueKind::Bool => Ok(Value::Bool(self.bool_value)),

            ValueKind::U8 => parse_int::<u8>(self.text.trim()).map(Value::U8),
            ValueKind::U16 => parse_int::<u16>(self.text.trim()).map(Value::U16),
            ValueKind::U32 => parse_int::<u32>(self.text.trim()).map(Value::U32),
            ValueKind::U64 => parse_int::<u64>(self.text.trim()).map(Value::U64),

            ValueKind::I8 => parse_int::<i8>(self.text.trim()).map(Value::I8),
            ValueKind::I16 => parse_int::<i16>(self.text.trim()).map(Value::I16),
            ValueKind::I32 => parse_int::<i32>(self.text.trim()).map(Value::I32),
            ValueKind::I64 => parse_int::<i64>(self.text.trim()).map(Value::I64),

            ValueKind::F32 => parse_float::<f32>(self.text.trim()).map(Value::F32),
            ValueKind::F64 => parse_float::<f64>(self.text.trim()).map(Value::F64),

            ValueKind::String => Ok(Value::String(self.text.clone())),
            ValueKind::Bytes => parse_hex_bytes(self.text.trim()).map(Value::Bytes),

            ValueKind::Array => {
                let mut out = Vec::with_capacity(self.items.len());
                for item in &mut self.items {
                    out.push(item.try_build_value()?);
                }
                Ok(Value::Array(out))
            }
            ValueKind::Map => {
                let mut out = Vec::with_capacity(self.entries.len());
                for (k, v) in &mut self.entries {
                    out.push((k.try_build_value()?, v.try_build_value()?));
                }
                Ok(Value::Map(out))
            }
        };

        if let Err(e) = &res {
            self.error = e.clone();
        }
        res
    }
}

pub(crate) fn ui_value_editor(ui: &mut egui::Ui, label: &str, state: &mut ValueEditorState) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(label);
            egui::ComboBox::from_id_salt(label)
                .selected_text(state.kind.label())
                .show_ui(ui, |ui| {
                    for kind in ALL_KINDS {
                        ui.selectable_value(&mut state.kind, kind, kind.label());
                    }
                });
        });

        match state.kind {
            ValueKind::Null | ValueKind::Unit => {}
            ValueKind::Bool => {
                ui.checkbox(&mut state.bool_value, "");
            }

            ValueKind::U8
            | ValueKind::U16
            | ValueKind::U32
            | ValueKind::U64
            | ValueKind::I8
            | ValueKind::I16
            | ValueKind::I32
            | ValueKind::I64
            | ValueKind::F32
            | ValueKind::F64
            | ValueKind::String
            | ValueKind::Bytes => {
                let hint = match state.kind {
                    ValueKind::Bytes => "hex (e.g. deadbeef)",
                    ValueKind::String => "text",
                    _ => "number",
                };
                ui.add(egui::TextEdit::singleline(&mut state.text).hint_text(hint));
            }

            ValueKind::Array => {
                for idx in 0..state.items.len() {
                    ui.push_id(idx, |ui| {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!("Item {idx}"));
                                if ui.button("Remove").clicked() {
                                    state.items.remove(idx);
                                }
                            });
                            if idx < state.items.len() {
                                ui_value_editor(ui, "", &mut state.items[idx]);
                            }
                        });
                    });
                }
                if ui.button("Add item").clicked() {
                    state.items.push(ValueEditorState::null());
                }
            }

            ValueKind::Map => {
                for idx in 0..state.entries.len() {
                    ui.push_id(idx, |ui| {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!("Entry {idx}"));
                                if ui.button("Remove").clicked() {
                                    state.entries.remove(idx);
                                }
                            });
                            if idx < state.entries.len() {
                                let (k, v) = &mut state.entries[idx];
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| ui_value_editor(ui, "Key", k));
                                    ui.separator();
                                    ui.vertical(|ui| ui_value_editor(ui, "Value", v));
                                });
                            }
                        });
                    });
                }
                if ui.button("Add entry").clicked() {
                    state
                        .entries
                        .push((ValueEditorState::null(), ValueEditorState::null()));
                }
            }
        }

        if !state.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &state.error);
        }
    });
}

pub(crate) fn ui_value_view(ui: &mut egui::Ui, label: &str, value: &Value) {
    let mut state = ValueEditorState::from_value(value);
    ui.add_enabled_ui(false, |ui| {
        ui_value_editor(ui, label, &mut state);
    });
}

const ALL_KINDS: [ValueKind; 17] = [
    ValueKind::Null,
    ValueKind::Unit,
    ValueKind::Bool,
    ValueKind::U8,
    ValueKind::U16,
    ValueKind::U32,
    ValueKind::U64,
    ValueKind::I8,
    ValueKind::I16,
    ValueKind::I32,
    ValueKind::I64,
    ValueKind::F32,
    ValueKind::F64,
    ValueKind::String,
    ValueKind::Bytes,
    ValueKind::Array,
    ValueKind::Map,
];

fn parse_int<T>(s: &str) -> Result<T, String>
where
    T: core::str::FromStr,
{
    s.parse::<T>().map_err(|_| format!("Invalid integer: {s}"))
}

fn parse_float<T>(s: &str) -> Result<T, String>
where
    T: core::str::FromStr,
{
    s.parse::<T>().map_err(|_| format!("Invalid float: {s}"))
}

fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    let mut hex = String::new();
    for ch in s.chars() {
        if ch.is_ascii_hexdigit() {
            hex.push(ch);
        } else if ch.is_ascii_whitespace() || ch == '_' {
            continue;
        } else {
            return Err(format!("Invalid hex character '{ch}'"));
        }
    }
    if hex.len() % 2 != 0 {
        return Err("Hex must have even length".to_string());
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let b = hex.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let hi = from_hex(b[i])?;
        let lo = from_hex(b[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn from_hex(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err("Invalid hex digit".to_string()),
    }
}
