use awrk_datex::value::Value;
use awrk_datex_schema::PrimitiveKind;
use eframe::egui;

use crate::schema_types::TypeKind;
use crate::value_editor::{ValueEditorState, ui_value_editor};

#[derive(Clone, Debug, Default)]
pub(crate) enum ScalarType {
    #[default]
    Bool,
    String,
    I64,
    U64,
    F64,
    Value,
}

#[derive(Clone, Debug)]
pub(crate) struct FieldState {
    pub(crate) scalar_type: ScalarType,
    pub(crate) text: String,
    pub(crate) bool_value: bool,
    pub(crate) value_state: ValueEditorState,
}

impl Default for FieldState {
    fn default() -> Self {
        Self {
            scalar_type: ScalarType::Value,
            text: String::new(),
            bool_value: false,
            value_state: ValueEditorState::null(),
        }
    }
}

pub(crate) fn ui_field_editor(ui: &mut egui::Ui, label: &str, state: &mut FieldState) {
    ui.push_id(label, |ui| match state.scalar_type {
        ScalarType::Value => {
            ui.group(|ui| {
                ui.label(label);
                ui_value_editor(ui, "", &mut state.value_state);
            });
        }
        _ => {
            ui.horizontal(|ui| {
                ui.label(label);
                match state.scalar_type {
                    ScalarType::Bool => {
                        ui.checkbox(&mut state.bool_value, "");
                    }
                    ScalarType::String => {
                        ui.text_edit_singleline(&mut state.text);
                    }
                    ScalarType::I64 | ScalarType::U64 | ScalarType::F64 => {
                        ui.text_edit_singleline(&mut state.text);
                    }
                    ScalarType::Value => {}
                }
            });
        }
    });
}

pub(crate) fn scalar_type_for_type_name(type_name: &str) -> ScalarType {
    let t = type_name.trim();
    // `schema_type_name` may return fully-qualified names (e.g. `core::primitive::u32`).
    // Normalize to a "base" identifier so we can pick a sensible scalar editor.
    let t = t.trim_start_matches('&').trim();
    let t = t.strip_prefix("mut ").unwrap_or(t).trim();
    let base = t.split('<').next().unwrap_or(t).trim();
    let base = base.rsplit("::").next().unwrap_or(base).trim();

    // Unwrap Option<T> to select a sensible editor for the inner type.
    // This keeps optional scalar args (common in RPCs) from showing up as generic Value/Null.
    if base == "Option" {
        if let Some(inner) = option_inner_type_name(t) {
            return scalar_type_for_type_name(&inner);
        }
        return ScalarType::Value;
    }

    if base == "bool" {
        return ScalarType::Bool;
    }
    if base == "str" {
        return ScalarType::String;
    }
    if base == "String" || t.ends_with("::String") {
        return ScalarType::String;
    }
    if matches!(base, "u8" | "u16" | "u32" | "u64" | "usize") {
        return ScalarType::U64;
    }
    if matches!(base, "i8" | "i16" | "i32" | "i64" | "isize") {
        return ScalarType::I64;
    }
    if matches!(base, "f32" | "f64") {
        return ScalarType::F64;
    }
    ScalarType::Value
}

fn option_inner_type_name(type_name: &str) -> Option<String> {
    // Extract the substring inside the outermost `Option<...>`.
    let s = type_name.trim();
    let start = s.find('<')?;
    let mut depth: i32 = 0;
    for (i, ch) in s.char_indices().skip(start) {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    let inner = s[start + 1..i].trim();
                    return (!inner.is_empty()).then(|| inner.to_string());
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn scalar_type_for_primitive(prim: PrimitiveKind) -> ScalarType {
    match prim {
        PrimitiveKind::Bool => ScalarType::Bool,
        PrimitiveKind::String => ScalarType::String,
        PrimitiveKind::Unsigned { .. } => ScalarType::U64,
        PrimitiveKind::Signed { .. } => ScalarType::I64,
        PrimitiveKind::Float { .. } => ScalarType::F64,
        PrimitiveKind::Null | PrimitiveKind::Unit => ScalarType::Value,
        PrimitiveKind::Bytes => ScalarType::Value,
    }
}

pub(crate) fn seed_form_from_existing(
    kind: &TypeKind,
    existing: Option<&Value>,
    out: &mut std::collections::BTreeMap<String, FieldState>,
) {
    match kind {
        TypeKind::Unit => {}
        TypeKind::Primitive { prim } => {
            let mut state = FieldState {
                scalar_type: scalar_type_for_primitive(*prim),
                text: String::new(),
                bool_value: false,
                value_state: ValueEditorState::null(),
            };
            if let Some(v) = existing {
                fill_field_state_from_value(&mut state, v);
            }
            out.insert("value".to_string(), state);
        }
        TypeKind::Struct { fields } => {
            for f in fields {
                let scalar = scalar_type_for_type_name(&f.type_name);
                let mut state = FieldState {
                    scalar_type: scalar.clone(),
                    text: String::new(),
                    bool_value: false,
                    value_state: ValueEditorState::null(),
                };
                if let Some(v) = existing.and_then(|v| wire_map_get_u64_key(v, f.field_id)) {
                    fill_field_state_from_value(&mut state, v);
                }
                out.insert(f.name.clone(), state);
            }
        }
        TypeKind::Tuple { items } => {
            for it in items {
                let key = it.index.to_string();
                let scalar = scalar_type_for_type_name(&it.type_name);
                let mut state = FieldState {
                    scalar_type: scalar.clone(),
                    text: String::new(),
                    bool_value: false,
                    value_state: ValueEditorState::null(),
                };

                if let Some(val) = existing.and_then(|v| match v {
                    Value::Array(arr) => arr.get(it.index as usize),
                    _ => None,
                }) {
                    fill_field_state_from_value(&mut state, val);
                }

                out.insert(key, state);
            }
        }
        TypeKind::Enum { .. } => {
            let mut state = FieldState {
                scalar_type: ScalarType::Value,
                text: String::new(),
                bool_value: false,
                value_state: ValueEditorState::null(),
            };
            if let Some(v) = existing {
                fill_field_state_from_value(&mut state, v);
            }
            out.insert("value".to_string(), state);
        }
        TypeKind::Other { .. } => {}
    }
}

fn fill_field_state_from_value(state: &mut FieldState, v: &Value) {
    match state.scalar_type {
        ScalarType::Bool => {
            if let Value::Bool(b) = v {
                state.bool_value = *b;
            }
        }
        ScalarType::String => {
            if let Value::String(s) = v {
                state.text = s.clone();
            } else {
                state.text = awrk_datex::text::format_value_compact(v);
            }
        }
        ScalarType::I64 | ScalarType::U64 | ScalarType::F64 => {
            state.text = match (state.scalar_type.clone(), v) {
                (ScalarType::I64, Value::I8(x)) => x.to_string(),
                (ScalarType::I64, Value::I16(x)) => x.to_string(),
                (ScalarType::I64, Value::I32(x)) => x.to_string(),
                (ScalarType::I64, Value::I64(x)) => x.to_string(),
                (ScalarType::U64, Value::U8(x)) => x.to_string(),
                (ScalarType::U64, Value::U16(x)) => x.to_string(),
                (ScalarType::U64, Value::U32(x)) => x.to_string(),
                (ScalarType::U64, Value::U64(x)) => x.to_string(),
                (ScalarType::F64, Value::F32(x)) => x.to_string(),
                (ScalarType::F64, Value::F64(x)) => x.to_string(),
                _ => awrk_datex::text::format_value_compact(v),
            };
        }
        ScalarType::Value => {
            state.value_state = ValueEditorState::from_value(v);
        }
    }
}

pub(crate) fn build_value_from_form(
    kind: &TypeKind,
    fields: &std::collections::BTreeMap<String, FieldState>,
) -> Result<Value, String> {
    match kind {
        TypeKind::Unit => Ok(Value::Unit),
        TypeKind::Primitive { .. } => {
            let state = fields
                .get("value")
                .ok_or_else(|| "Missing field: value".to_string())?;
            field_state_to_value(state)
        }
        TypeKind::Struct {
            fields: schema_fields,
        } => {
            let mut entries = Vec::with_capacity(schema_fields.len());
            for f in schema_fields {
                let Some(state) = fields.get(&f.name) else {
                    continue;
                };
                let v = field_state_to_value(state)?;
                entries.push((Value::U64(f.field_id), v));
            }
            Ok(Value::Map(entries))
        }
        TypeKind::Tuple { items } => {
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                let key = it.index.to_string();
                let Some(state) = fields.get(&key) else {
                    return Err(format!("Missing tuple item {key}"));
                };
                out.push(field_state_to_value(state)?);
            }
            Ok(Value::Array(out))
        }
        TypeKind::Enum { .. } => {
            let state = fields
                .get("value")
                .ok_or_else(|| "Missing field: value".to_string())?;
            field_state_to_value(state)
        }
        TypeKind::Other { kind } => Err(format!("No form builder available for type kind: {kind}")),
    }
}

pub(crate) fn field_state_to_value(state: &FieldState) -> Result<Value, String> {
    match state.scalar_type {
        ScalarType::Bool => Ok(Value::Bool(state.bool_value)),
        ScalarType::String => {
            if state.text.trim().is_empty() {
                Ok(Value::Null)
            } else {
                Ok(Value::String(state.text.clone()))
            }
        }
        ScalarType::I64 => {
            if state.text.trim().is_empty() {
                return Ok(Value::Null);
            }
            let v: i64 = state
                .text
                .trim()
                .parse()
                .map_err(|_| format!("Invalid integer: {}", state.text))?;
            Ok(Value::I64(v))
        }
        ScalarType::U64 => {
            if state.text.trim().is_empty() {
                return Ok(Value::Null);
            }
            let v: u64 = state
                .text
                .trim()
                .parse()
                .map_err(|_| format!("Invalid unsigned integer: {}", state.text))?;
            Ok(Value::U64(v))
        }
        ScalarType::F64 => {
            if state.text.trim().is_empty() {
                return Ok(Value::Null);
            }
            let v: f64 = state
                .text
                .trim()
                .parse()
                .map_err(|_| format!("Invalid float: {}", state.text))?;
            Ok(Value::F64(v))
        }
        ScalarType::Value => {
            let mut vs = state.value_state.clone();
            vs.try_build_value()
        }
    }
}

fn wire_map_get_u64_key<'a>(value: &'a Value, key: u64) -> Option<&'a Value> {
    let Value::Map(entries) = value else {
        return None;
    };
    entries
        .iter()
        .find_map(|(k, v)| matches!(k, Value::U64(u) if *u == key).then_some(v))
}
