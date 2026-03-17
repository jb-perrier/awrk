use crate::forms::{
    FieldState, ScalarType, build_value_from_form, field_state_to_value, scalar_type_for_primitive,
    scalar_type_for_type_name, seed_form_from_existing, structured_kind_for_type_name,
    ui_field_editor,
};
use crate::model::{TreeData, build_tree};
use crate::schema_types::{ProcInfo, TypeInfo, TypeKind};
use crate::value_editor::{ValueEditorState, ValueKind, ui_value_editor, ui_value_view};
use crate::worker::{WorkerRequest, WorkerResponse, start_worker};
use awrk_datex::value::Value;
use awrk_world::rpc::{EntityInfo, RpcTrace};
use awrk_world_ecs::{Name, Parent};
use eframe::egui;
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Entities,
    Types,
    RuntimeRpcs,
    RpcTraces,
}

pub struct ExplorerUiApp {
    host: String,
    port_input: String,

    worker_tx: mpsc::Sender<WorkerRequest>,
    worker_rx: mpsc::Receiver<WorkerResponse>,
    refresh_in_flight: bool,

    status: String,
    auto_refresh: bool,
    refresh_interval: Duration,
    last_refresh: Option<Instant>,
    active_tab: Tab,

    entities: Vec<EntityInfo>,
    types: Vec<TypeInfo>,
    procs: Vec<ProcInfo>,

    selected_entity: Option<u64>,
    selected_component: Option<String>,

    show_name_parent_components: bool,

    component_type_selected: String,
    component_fields: std::collections::BTreeMap<String, FieldState>,
    component_form_error: String,

    patch_field_selected: String,
    patch_field_value: FieldState,

    rpc_proc_input: String,
    rpc_args_value: ValueEditorState,
    rpc_use_raw_value: bool,
    rpc_form_fields: std::collections::BTreeMap<String, FieldState>,
    rpc_form_error: String,
    rpc_result_value: Option<Value>,

    rpc_traces: Vec<RpcTrace>,

    visible_entities: Vec<u64>,

    expanded_entities: Vec<u64>,
}

impl Default for ExplorerUiApp {
    fn default() -> Self {
        let (worker_tx, worker_rx) = start_worker();
        Self {
            host: "127.0.0.1".to_string(),
            port_input: "7777".to_string(),

            worker_tx,
            worker_rx,
            refresh_in_flight: false,

            status: "Ready".to_string(),
            auto_refresh: true,
            refresh_interval: Duration::from_millis(150),
            last_refresh: None,
            active_tab: Tab::Entities,
            entities: Vec::new(),
            types: Vec::new(),
            procs: Vec::new(),
            selected_entity: None,
            selected_component: None,

            show_name_parent_components: false,

            component_type_selected: String::new(),
            component_fields: std::collections::BTreeMap::new(),
            component_form_error: String::new(),

            patch_field_selected: String::new(),
            patch_field_value: FieldState {
                scalar_type: ScalarType::Value,
                value_state: ValueEditorState::null(),
                ..Default::default()
            },

            rpc_proc_input: "awrk.list_entities".to_string(),
            rpc_args_value: ValueEditorState::from_value(&Value::Map(Vec::new())),
            rpc_use_raw_value: false,
            rpc_form_fields: std::collections::BTreeMap::new(),
            rpc_form_error: String::new(),
            rpc_result_value: None,

            rpc_traces: Vec::new(),

            visible_entities: Vec::new(),

            expanded_entities: Vec::new(),
        }
    }
}

impl Drop for ExplorerUiApp {
    fn drop(&mut self) {
        let _ = self.worker_tx.send(WorkerRequest::Quit);
    }
}

impl eframe::App for ExplorerUiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_worker_responses();

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Host:");
                ui.text_edit_singleline(&mut self.host);
                ui.label("Port:");
                ui.text_edit_singleline(&mut self.port_input);
                if ui.button("Win 7777").clicked() {
                    self.host = "127.0.0.1".to_string();
                    self.port_input = "7777".to_string();
                }
                if ui.button("Example 7780").clicked() {
                    self.host = "127.0.0.1".to_string();
                    self.port_input = "7780".to_string();
                }
                if ui.button("Refresh").clicked() {
                    self.request_refresh();
                }
                if ui.button("Spawn Empty (Low-level)").clicked() {
                    self.spawn_empty();
                }
                ui.checkbox(&mut self.auto_refresh, "Auto");
                ui.label(&self.status);
            });
            ui.label(
                "Low-level runtime explorer: direct awrk.* RPCs are for tooling/debugging. Domain workflows should usually be driven by entity/component state and bridge markers.",
            );
        });

        egui::TopBottomPanel::top("tabs_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, Tab::Entities, "Entities");
                ui.selectable_value(&mut self.active_tab, Tab::Types, "Types");
                ui.selectable_value(&mut self.active_tab, Tab::RuntimeRpcs, "Runtime RPCs");
                ui.selectable_value(&mut self.active_tab, Tab::RpcTraces, "RPC Traces");
            });
        });

        match self.active_tab {
            Tab::Entities => self.ui_entities(ctx),
            Tab::Types => self.ui_types(ctx),
            Tab::RuntimeRpcs => self.ui_procedures(ctx),
            Tab::RpcTraces => self.ui_requests(ctx),
        }

        // Run this after UI so `visible_entities` is up-to-date.
        self.maybe_auto_refresh(ctx);
    }
}

impl ExplorerUiApp {
    const TRACE_MAX: usize = 2000;
    const TRACE_WINDOW_SECS: u64 = 5;

    fn render_component_value(&self, type_name: &str, value: Option<&Value>) -> String {
        match value {
            Some(value) => self.render_value_for_type_name(type_name, value, 0),
            None => "<none>".to_string(),
        }
    }

    fn render_value_for_type_name(&self, type_name: &str, value: &Value, depth: usize) -> String {
        if depth >= 6 {
            return awrk_datex::text::format_value_compact(value);
        }

        let Some(type_info) = self.types.iter().find(|t| t.type_name == type_name) else {
            return awrk_datex::text::format_value_compact(value);
        };

        self.render_value_for_kind(&type_info.kind, value, depth)
    }

    fn render_value_for_kind(&self, kind: &TypeKind, value: &Value, depth: usize) -> String {
        match (kind, value) {
            (TypeKind::Struct { fields }, Value::Map(entries)) => {
                let mut rendered_fields = Vec::with_capacity(entries.len());
                for (key, field_value) in entries {
                    let field_id = match key {
                        Value::U8(value) => Some(*value as u64),
                        Value::U16(value) => Some(*value as u64),
                        Value::U32(value) => Some(*value as u64),
                        Value::U64(value) => Some(*value),
                        _ => None,
                    };

                    if let Some(field_id) = field_id
                        && let Some(field) = fields.iter().find(|field| field.field_id == field_id)
                    {
                        rendered_fields.push(format!(
                            "{}: {}",
                            field.name,
                            self.render_value_for_type_name(
                                &field.type_name,
                                field_value,
                                depth + 1,
                            )
                        ));
                        continue;
                    }

                    rendered_fields.push(format!(
                        "{}: {}",
                        awrk_datex::text::format_value_compact(key),
                        awrk_datex::text::format_value_compact(field_value)
                    ));
                }
                format!("{{{}}}", rendered_fields.join(", "))
            }
            (TypeKind::Tuple { items }, Value::Array(entries)) => {
                let rendered_items: Vec<String> = entries
                    .iter()
                    .enumerate()
                    .map(|(index, item_value)| {
                        if let Some(item) = items.get(index) {
                            self.render_value_for_type_name(&item.type_name, item_value, depth + 1)
                        } else {
                            awrk_datex::text::format_value_compact(item_value)
                        }
                    })
                    .collect();
                format!("({})", rendered_items.join(", "))
            }
            (TypeKind::Enum { variants, .. }, Value::Map(entries)) if entries.len() == 1 => {
                let (variant_key, payload) = &entries[0];
                let variant_index = match variant_key {
                    Value::U8(value) => Some(*value as usize),
                    Value::U16(value) => Some(*value as usize),
                    Value::U32(value) => Some(*value as usize),
                    Value::U64(value) => Some(*value as usize),
                    _ => None,
                };

                if let Some(variant) = variant_index.and_then(|index| variants.get(index)) {
                    return match &variant.payload_type_name {
                        Some(type_name) => format!(
                            "{}({})",
                            variant.name,
                            self.render_value_for_type_name(type_name, payload, depth + 1)
                        ),
                        None => variant.name.clone(),
                    };
                }

                awrk_datex::text::format_value_compact(value)
            }
            _ => awrk_datex::text::format_value_compact(value),
        }
    }

    fn default_value_for_type_name(types: &[TypeInfo], type_name: &str) -> Value {
        Self::default_value_for_type_name_inner(types, type_name, 0, &mut HashSet::new())
    }

    fn default_value_for_type_name_inner(
        types: &[TypeInfo],
        type_name: &str,
        depth: usize,
        visiting: &mut HashSet<String>,
    ) -> Value {
        if depth >= 8 {
            return Value::Null;
        }

        let t = type_name.trim();
        if t.is_empty() {
            return Value::Null;
        }

        let t2 = t.trim_start_matches('&').trim();
        let t2 = t2.strip_prefix("mut ").unwrap_or(t2).trim();
        let t2 = t2.split('<').next().unwrap_or(t2).trim();
        let base = t2.rsplit("::").next().unwrap_or(t2).trim();

        // Fast-path for common primitives by name.
        match base {
            "bool" => return Value::Bool(false),
            "str" => return Value::String(String::new()),
            "String" => return Value::String(String::new()),
            "u8" => return Value::U8(0),
            "u16" => return Value::U16(0),
            "u32" => return Value::U32(0),
            "u64" | "usize" => return Value::U64(0),
            "i8" => return Value::I8(0),
            "i16" => return Value::I16(0),
            "i32" => return Value::I32(0),
            "i64" | "isize" => return Value::I64(0),
            "f32" => return Value::F32(0.0),
            "f64" => return Value::F64(0.0),
            _ => {}
        }

        if !visiting.insert(t.to_string()) {
            return Value::Null;
        }

        let resolved = types
            .iter()
            .find(|ty| ty.type_name == t)
            .map(|ty| Self::default_value_for_kind_inner(types, &ty.kind, depth + 1, visiting))
            .unwrap_or(Value::Null);

        visiting.remove(t);
        resolved
    }

    fn default_value_for_kind(types: &[TypeInfo], kind: &TypeKind) -> Value {
        Self::default_value_for_kind_inner(types, kind, 0, &mut HashSet::new())
    }

    fn default_value_for_primitive(prim: awrk_datex_schema::PrimitiveKind) -> Value {
        match prim {
            awrk_datex_schema::PrimitiveKind::Bool => Value::Bool(false),
            awrk_datex_schema::PrimitiveKind::String => Value::String(String::new()),
            awrk_datex_schema::PrimitiveKind::Unsigned { bits } => match bits {
                8 => Value::U8(0),
                16 => Value::U16(0),
                32 => Value::U32(0),
                _ => Value::U64(0),
            },
            awrk_datex_schema::PrimitiveKind::Signed { bits } => match bits {
                8 => Value::I8(0),
                16 => Value::I16(0),
                32 => Value::I32(0),
                _ => Value::I64(0),
            },
            awrk_datex_schema::PrimitiveKind::Float { bits } => match bits {
                32 => Value::F32(0.0),
                _ => Value::F64(0.0),
            },
            awrk_datex_schema::PrimitiveKind::Bytes => Value::Bytes(Vec::new()),
            awrk_datex_schema::PrimitiveKind::Null => Value::Null,
            awrk_datex_schema::PrimitiveKind::Unit => Value::Unit,
        }
    }

    fn default_value_for_kind_inner(
        types: &[TypeInfo],
        kind: &TypeKind,
        depth: usize,
        visiting: &mut HashSet<String>,
    ) -> Value {
        if depth >= 8 {
            return Value::Null;
        }

        match kind {
            TypeKind::Unit => Value::Unit,
            TypeKind::Primitive { prim } => Self::default_value_for_primitive(*prim),
            TypeKind::Struct { fields } => Value::Map(
                fields
                    .iter()
                    .map(|f| {
                        (
                            Value::U64(f.field_id),
                            Self::default_value_for_type_name_inner(
                                types,
                                &f.type_name,
                                depth + 1,
                                visiting,
                            ),
                        )
                    })
                    .collect(),
            ),
            TypeKind::Tuple { items } => Value::Array(
                items
                    .iter()
                    .map(|it| {
                        Self::default_value_for_type_name_inner(
                            types,
                            &it.type_name,
                            depth + 1,
                            visiting,
                        )
                    })
                    .collect(),
            ),
            TypeKind::Enum { variants, repr } => {
                let Some(v0) = variants.first() else {
                    return Value::Null;
                };

                match repr {
                    awrk_datex_schema::EnumRepr::IndexKeyedSingleEntryMap => {
                        let key = Value::U64(v0.index as u64);
                        let payload = match &v0.payload_type_name {
                            None => Value::Bool(true),
                            Some(type_name) => Self::default_value_for_type_name_inner(
                                types,
                                type_name,
                                depth + 1,
                                visiting,
                            ),
                        };
                        Value::Map(vec![(key, payload)])
                    }
                }
            }
            TypeKind::Other { .. } => Value::Null,
        }
    }

    fn seed_field_state_defaults(
        types: &[TypeInfo],
        state: &mut FieldState,
        scalar: ScalarType,
        value_type_name: Option<&str>,
        is_optional: bool,
        reset_values: bool,
    ) {
        if reset_values {
            state.text.clear();
            state.bool_value = false;
            state.value_state = ValueEditorState::null();
        }

        if !matches!(scalar, ScalarType::Value) {
            state.typed_kind = None;
            state.typed_fields.clear();
        }

        match scalar {
            ScalarType::I64 | ScalarType::U64 => {
                if !is_optional && state.text.trim().is_empty() {
                    state.text = "0".to_string();
                }
            }
            ScalarType::F64 => {
                if !is_optional && state.text.trim().is_empty() {
                    state.text = "0.0".to_string();
                }
            }
            ScalarType::Bool | ScalarType::String => {}
            ScalarType::Value => {
                let next_typed_kind = value_type_name
                    .and_then(|type_name| structured_kind_for_type_name(types, type_name));
                let kind_changed = state.typed_kind.as_ref() != next_typed_kind.as_ref();

                if kind_changed {
                    state.typed_fields.clear();
                }
                state.typed_kind = next_typed_kind;

                if let Some(kind) = state.typed_kind.clone() {
                    let should_seed = reset_values || kind_changed || state.typed_fields.is_empty();
                    if should_seed {
                        seed_form_from_existing(types, &kind, None, &mut state.typed_fields);
                    }
                    state.value_state = ValueEditorState::null();
                } else {
                    let should_seed =
                        reset_values || matches!(state.value_state.kind, ValueKind::Null);
                    if should_seed {
                        state.value_state = ValueEditorState::null();
                    }
                }
            }
        }
    }

    fn merge_entities_by_revision(&mut self, fresh: Vec<EntityInfo>) {
        let mut old_by_id: HashMap<u64, EntityInfo> = HashMap::with_capacity(self.entities.len());
        for e in std::mem::take(&mut self.entities) {
            old_by_id.insert(e.entity, e);
        }

        let mut merged: Vec<EntityInfo> = Vec::with_capacity(fresh.len());
        for e in fresh {
            match old_by_id.remove(&e.entity) {
                Some(old) if old.revision == e.revision => merged.push(old),
                _ => merged.push(e),
            }
        }

        if let Some(sel) = self.selected_entity
            && !merged.iter().any(|e| e.entity == sel)
        {
            self.selected_entity = None;
            self.selected_component = None;
        }

        self.entities = merged;
    }

    fn ui_entities(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("entities_tree")
            .resizable(true)
            .min_width(260.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Scene Tree");
                    if ui.button("New Root Entity").clicked() {
                        self.spawn_entity(None);
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let clip = ui.clip_rect();
                    let mut visible = Vec::new();
                    let mut expanded = Vec::new();
                    let tree = build_tree(&self.entities);
                    for root in &tree.roots {
                        self.ui_entity_node(ui, *root, &tree, clip, &mut visible, &mut expanded);
                    }

                    // Simple rule: if a node is expanded, proactively load its direct children.
                    let mut fetch = visible;
                    for id in &expanded {
                        if let Some(kids) = tree.children.get(id) {
                            fetch.extend(kids.iter().copied());
                        }
                    }
                    fetch.sort_unstable();
                    fetch.dedup();
                    self.visible_entities = fetch;

                    // If the user just expanded something, request a refresh immediately so
                    // children details load without waiting for the auto-refresh interval.
                    expanded.sort_unstable();
                    expanded.dedup();
                    let mut did_expand = false;
                    {
                        let mut prev_i = 0;
                        for id in &expanded {
                            while prev_i < self.expanded_entities.len()
                                && self.expanded_entities[prev_i] < *id
                            {
                                prev_i += 1;
                            }
                            if prev_i >= self.expanded_entities.len()
                                || self.expanded_entities[prev_i] != *id
                            {
                                did_expand = true;
                                break;
                            }
                        }
                    }
                    self.expanded_entities = expanded;
                    if did_expand {
                        self.request_refresh();
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Inspector");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut self.show_name_parent_components, "Show Name/Parent");
                });
            });
            ui.label(
                "Inspect runtime entities/components here, and use explicit domain RPCs for app-facing workflows.",
            );
            ui.separator();

            if let Some(entity_id) = self.selected_entity {
                if let Some(type_name) = self.selected_component.clone() {
                    self.ui_component_page(ui, entity_id, &type_name);
                } else {
                    self.ui_entity_inspector(ui, entity_id);
                }
            } else {
                ui.label("Select an entity to inspect components.");
            }
        });
    }

    fn ui_types(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Registered Types");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for t in &self.types {
                    ui.collapsing(&t.type_name, |ui| {
                        self.ui_type_kind(ui, &t.kind);
                    });
                }
            });
        });
    }

    fn ui_procedures(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Runtime RPCs");
            ui.separator();

            ui.label("Available low-level runtime/tooling procedures:");
            ui.label(
                "Use these for inspection and debugging. App-facing flows should usually be modeled through explicit domain RPCs instead of runtime-level entity mutation calls.",
            );
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for p in &self.procs {
                        ui.collapsing(&p.name, |ui| {
                            ui.label("Args:");
                            self.ui_type_kind(ui, &p.args);
                            ui.label("Result:");
                            self.ui_type_kind(ui, &p.result);
                        });
                    }
                });

            ui.separator();
            ui.heading("RPC Console");

            let before_proc = self.rpc_proc_input.clone();
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("rpc_proc")
                    .selected_text(&self.rpc_proc_input)
                    .show_ui(ui, |ui| {
                        for p in &self.procs {
                            ui.selectable_value(&mut self.rpc_proc_input, p.name.clone(), &p.name);
                        }
                    });

                if ui.button("Invoke").clicked() {
                    self.invoke_rpc();
                }
            });

            if self.rpc_proc_input != before_proc {
                self.rebuild_rpc_form(true);
            }

            let selected_args_kind = self
                .procs
                .iter()
                .find(|p| p.name == self.rpc_proc_input)
                .map(|p| p.args.clone());

            match selected_args_kind {
                Some(TypeKind::Other { .. }) | None => {
                    ui.label("Args:");
                    egui::ScrollArea::vertical()
                        .id_salt("rpc_args_scroll")
                        .max_height(ui.available_height().clamp(160.0, 320.0))
                        .show(ui, |ui| {
                            ui_value_editor(ui, "Args", &mut self.rpc_args_value);
                        });
                }
                Some(
                    kind @ (TypeKind::Unit
                    | TypeKind::Primitive { .. }
                    | TypeKind::Struct { .. }
                    | TypeKind::Tuple { .. }
                    | TypeKind::Enum { .. }),
                ) => {
                    ui.horizontal(|ui| {
                        ui.label("Args:");
                        ui.checkbox(&mut self.rpc_use_raw_value, "Raw Value");
                    });

                    if self.rpc_use_raw_value {
                        egui::ScrollArea::vertical()
                            .id_salt("rpc_args_scroll")
                            .max_height(ui.available_height().clamp(160.0, 320.0))
                            .show(ui, |ui| {
                                ui_value_editor(ui, "Args", &mut self.rpc_args_value);
                            });
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("rpc_args_scroll")
                            .max_height(ui.available_height().clamp(160.0, 320.0))
                            .show(ui, |ui| {
                                if !self.rpc_form_error.is_empty() {
                                    ui.colored_label(egui::Color32::LIGHT_RED, &self.rpc_form_error);
                                }

                                match &kind {
                                    TypeKind::Unit => {
                                        ui.label("<no args>");
                                    }
                                    TypeKind::Primitive { prim } => {
                                        let scalar = scalar_type_for_primitive(*prim);
                                        let entry = self
                                            .rpc_form_fields
                                            .entry("value".to_string())
                                            .or_insert_with(|| FieldState {
                                                scalar_type: scalar.clone(),
                                                ..Default::default()
                                            });
                                        entry.scalar_type = scalar;
                                        ui_field_editor(ui, "Value", entry);
                                    }
                                    TypeKind::Struct { fields } => {
                                        if fields.is_empty() {
                                            ui.label("<empty struct>");
                                        } else {
                                            for f in fields {
                                                let entry = self
                                                    .rpc_form_fields
                                                    .entry(f.name.clone())
                                                    .or_insert_with(|| FieldState {
                                                        scalar_type: scalar_type_for_type_name(
                                                            &f.type_name,
                                                        ),
                                                        ..Default::default()
                                                    });
                                                ui_field_editor(ui, &f.name, entry);
                                            }
                                        }
                                    }
                                    TypeKind::Tuple { items } => {
                                        if items.is_empty() {
                                            ui.label("<empty tuple>");
                                        } else {
                                            for it in items {
                                                let name = it.index.to_string();
                                                let entry = self
                                                    .rpc_form_fields
                                                    .entry(name.clone())
                                                    .or_insert_with(|| FieldState {
                                                        scalar_type: scalar_type_for_type_name(
                                                            &it.type_name,
                                                        ),
                                                        ..Default::default()
                                                    });
                                                ui_field_editor(ui, &format!("[{name}]"), entry);
                                            }
                                        }
                                    }
                                    TypeKind::Enum { .. } => {
                                        let entry = self
                                            .rpc_form_fields
                                            .entry("value".to_string())
                                            .or_insert_with(|| FieldState {
                                                scalar_type: ScalarType::Value,
                                                ..Default::default()
                                            });
                                        entry.scalar_type = ScalarType::Value;
                                        ui_field_editor(ui, "Value", entry);
                                    }
                                    TypeKind::Other { .. } => {}
                                }
                            });
                    }
                }
            }

            ui.label("Result:");
            if let Some(v) = &self.rpc_result_value {
                ui_value_view(ui, "Result", v);
            } else {
                ui.label("<none>");
            }
        });
    }

    fn ui_requests(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("RPC Traces");
                ui.label(format!("window: {}s", Self::TRACE_WINDOW_SECS));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear").clicked() {
                        self.rpc_traces.clear();
                    }
                    ui.label(format!("{} calls", self.rpc_traces.len()));
                });
            });
            ui.separator();

            #[derive(Default, Clone, Copy)]
            struct Agg {
                calls: u64,
                ok: u64,
                fail: u64,
                win_calls: u64,
                total_us: u64,
                max_us: u64,
                total_req_bytes: u64,
                total_resp_bytes: u64,
                max_resp_bytes: u64,
                resp_calls: u64,
                total_resp_deser_us: u64,
                resp_deser_calls: u64,
            }

            let cutoff =
                std::time::Instant::now() - std::time::Duration::from_secs(Self::TRACE_WINDOW_SECS);
            let mut by_proc: std::collections::BTreeMap<&str, Agg> =
                std::collections::BTreeMap::new();

            // Lifetime totals (over retained trace buffer): calls/ok/fail.
            for t in &self.rpc_traces {
                let a = by_proc.entry(t.proc.as_str()).or_default();
                a.calls += 1;
                if t.ok {
                    a.ok += 1;
                } else {
                    a.fail += 1;
                }
            }

            // Windowed metrics: timing + sizes + deser.
            for t in self.rpc_traces.iter().filter(|t| t.at >= cutoff) {
                let a = by_proc.entry(t.proc.as_str()).or_default();
                a.win_calls += 1;
                a.total_us = a.total_us.saturating_add(t.duration_us);
                a.max_us = a.max_us.max(t.duration_us);
                a.total_req_bytes = a.total_req_bytes.saturating_add(t.request_bytes);
                if let Some(b) = t.response_bytes {
                    a.total_resp_bytes = a.total_resp_bytes.saturating_add(b);
                    a.max_resp_bytes = a.max_resp_bytes.max(b);
                    a.resp_calls += 1;
                }
                if let Some(us) = t.response_decode_us {
                    a.total_resp_deser_us = a.total_resp_deser_us.saturating_add(us);
                    a.resp_deser_calls += 1;
                }
            }

            fn ms(us: u64) -> f64 {
                us as f64 / 1000.0
            }

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    egui::Grid::new("requests_totals")
                        .striped(true)
                        .spacing([12.0, 4.0])
                        .show(ui, |ui| {
                            ui.monospace("proc");
                            ui.monospace("calls");
                            ui.monospace("ok");
                            ui.monospace("fail");
                            ui.monospace("avg_ms");
                            ui.monospace("max_ms");
                            ui.monospace("avg_resp_deser_ms");
                            ui.monospace("avg_reqB");
                            ui.monospace("avg_respB");
                            ui.monospace("max_respB");
                            ui.end_row();

                            for (proc, a) in by_proc {
                                let avg_us = if a.win_calls == 0 {
                                    0.0
                                } else {
                                    a.total_us as f64 / a.win_calls as f64
                                };
                                let avg_req_b = if a.win_calls == 0 {
                                    0
                                } else {
                                    a.total_req_bytes.checked_div(a.win_calls).unwrap_or(0)
                                };
                                let avg_resp_b = if a.resp_calls == 0 {
                                    0
                                } else {
                                    a.total_resp_bytes.checked_div(a.resp_calls).unwrap_or(0)
                                };
                                let avg_resp_deser_us = if a.resp_deser_calls == 0 {
                                    0.0
                                } else {
                                    a.total_resp_deser_us as f64 / a.resp_deser_calls as f64
                                };
                                ui.monospace(proc);
                                ui.monospace(format!("{:>5}", a.calls));
                                ui.monospace(format!("{:>5}", a.ok));
                                if a.fail == 0 {
                                    ui.monospace(format!("{:>5}", a.fail));
                                } else {
                                    ui.colored_label(
                                        egui::Color32::LIGHT_RED,
                                        format!("{:>5}", a.fail),
                                    );
                                }
                                ui.monospace(format!("{:>9.3}", avg_us / 1000.0));
                                ui.monospace(format!("{:>9.3}", ms(a.max_us)));
                                ui.monospace(format!("{:>16.3}", avg_resp_deser_us / 1000.0));
                                ui.monospace(format!("{:>8}", avg_req_b));
                                ui.monospace(format!("{:>8}", avg_resp_b));
                                ui.monospace(format!("{:>9}", a.max_resp_bytes));
                                ui.end_row();
                            }
                        });
                });
        });
    }

    fn ui_entity_node(
        &mut self,
        ui: &mut egui::Ui,
        id: u64,
        tree: &TreeData,
        clip: egui::Rect,
        visible: &mut Vec<u64>,
        expanded: &mut Vec<u64>,
    ) {
        let Some(node) = tree.nodes.get(&id) else {
            ui.label(format!("Entity {id}"));
            return;
        };

        let mut pending_select_component_type: Option<String> = None;
        let mut pending_spawn_child = false;
        let mut pending_despawn = false;

        let label = if let Some(name) = node.name.as_deref() {
            format!("{name} ({id})")
        } else {
            format!("Entity {id}")
        };

        let response = egui::CollapsingHeader::new(label)
            .id_salt(id)
            .default_open(false)
            .show(ui, |ui| {
                let entity = self
                    .entities
                    .get(node.entity_index)
                    .expect("entity index valid");
                for c in &entity.components {
                    if !self.show_name_parent_components
                        && (c.type_name == std::any::type_name::<Name>()
                            || c.type_name == std::any::type_name::<Parent>())
                    {
                        continue;
                    }
                    let rendered = self.render_component_value(&c.type_name, c.value.as_ref());
                    let selected = self.selected_entity == Some(id)
                        && self.selected_component.as_deref() == Some(c.type_name.as_str());
                    if ui
                        .selectable_label(selected, format!("• {} = {}", c.type_name, rendered))
                        .clicked()
                    {
                        pending_select_component_type = Some(c.type_name.clone());
                    }
                }

                if let Some(kids) = tree.children.get(&id) {
                    for child in kids {
                        self.ui_entity_node(ui, *child, tree, clip, visible, expanded);
                    }
                }
            });

        let header_response = response.header_response.clone();
        header_response.context_menu(|ui| {
            if ui.button("Add Child Entity").clicked() {
                pending_spawn_child = true;
                ui.close_menu();
            }
            if ui.button("Despawn Entity").clicked() {
                pending_despawn = true;
                ui.close_menu();
            }
        });

        if response.body_response.is_some() {
            expanded.push(id);
        }

        if response.header_response.rect.intersects(clip) {
            visible.push(id);
        }

        if header_response.clicked() {
            self.select_entity(id);
        }

        if let Some(type_name) = pending_select_component_type {
            self.select_component(id, type_name);
        }

        if pending_spawn_child {
            if !self.expanded_entities.contains(&id) {
                self.expanded_entities.push(id);
            }
            self.spawn_entity(Some(id));
        }

        if pending_despawn {
            self.despawn_entity(id);
        }
    }

    fn ui_component_page(&mut self, ui: &mut egui::Ui, entity_id: u64, type_name: &str) {
        if self.component_type_selected != type_name {
            self.component_type_selected = type_name.to_string();
            self.rebuild_component_form(entity_id);
        }

        ui.horizontal(|ui| {
            if ui.button("Back To Entity").clicked() {
                self.selected_component = None;
            }
            ui.heading("Component");
            ui.label(type_name);
        });
        ui.label(format!("Entity {entity_id}"));
        ui.separator();

        if let Some(entity) = self.entities.iter().find(|e| e.entity == entity_id)
            && let Some(component) = entity.components.iter().find(|c| c.type_name == type_name)
        {
            ui.label("Current Value:");
            if let Some(value) = &component.value {
                ui.label(self.render_component_value(type_name, Some(value)));
                ui.separator();
                ui_value_view(ui, "Value", value);
            } else {
                ui.label("<none>");
            }
        }

        ui.separator();
        self.ui_selected_component_editor(ui, entity_id);
    }

    fn ui_entity_inspector(&mut self, ui: &mut egui::Ui, entity_id: u64) {
        ui.horizontal(|ui| {
            ui.label(format!("Entity {entity_id}"));
            if ui.button("Despawn").clicked() {
                self.despawn_entity(entity_id);
            }
            if ui.button("Refresh").clicked() {
                self.request_refresh();
            }
        });
        ui.separator();

        let mut pending_select_component_type: Option<String> = None;
        let entity = self.entities.iter().find(|e| e.entity == entity_id);

        if let Some(entity) = entity {
            ui.label("Components:");
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for c in &entity.components {
                        if !self.show_name_parent_components
                            && (c.type_name == std::any::type_name::<Name>()
                                || c.type_name == std::any::type_name::<Parent>())
                        {
                            continue;
                        }
                        let selected = self
                            .selected_component
                            .as_deref()
                            .map(|v| v == c.type_name)
                            .unwrap_or(false);
                        let label = format!(
                            "{} = {}",
                            c.type_name,
                            self.render_component_value(&c.type_name, c.value.as_ref())
                        );
                        if ui.selectable_label(selected, label).clicked() {
                            pending_select_component_type = Some(c.type_name.clone());
                        }
                    }
                });
        }

        if let Some(type_name) = pending_select_component_type {
            self.selected_component = Some(type_name.clone());
            self.component_type_selected = type_name;
            self.rebuild_component_form(entity_id);
        }

        ui.separator();
        let before_type = self.component_type_selected.clone();
        ui.horizontal(|ui| {
            ui.label("Component Type:");
            egui::ComboBox::from_id_salt("component_type")
                .selected_text(if self.component_type_selected.is_empty() {
                    "<select>"
                } else {
                    self.component_type_selected.as_str()
                })
                .show_ui(ui, |ui| {
                    for t in self.types.iter().filter(|t| t.caps.is_component) {
                        ui.selectable_value(
                            &mut self.component_type_selected,
                            t.type_name.clone(),
                            &t.type_name,
                        );
                    }
                });
        });

        if self.component_type_selected != before_type {
            self.rebuild_component_form(entity_id);
        }

        if !self.component_form_error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.component_form_error);
        }

        let selected_type = self
            .types
            .iter()
            .find(|t| t.type_name == self.component_type_selected);

        if selected_type.is_some() {
            ui.separator();
            ui.label("Component Editor:");
            self.ui_selected_component_editor(ui, entity_id);
        } else {
            ui.separator();
            ui.label("Select a component type to edit.");
        }
    }

    fn ui_selected_component_editor(&mut self, ui: &mut egui::Ui, entity_id: u64) {
        if !self.component_form_error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.component_form_error);
        }

        let selected_type = self
            .types
            .iter()
            .find(|t| t.type_name == self.component_type_selected);

        let Some(selected_type) = selected_type else {
            ui.label("Select a component type to edit.");
            return;
        };

        let kind = selected_type.kind.clone();
        let caps = selected_type.caps;
        self.ui_component_form(ui, &kind);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(caps.can_write, egui::Button::new("Set Component"))
                .clicked()
            {
                self.set_component(entity_id);
            }
            if ui
                .add_enabled(caps.can_remove, egui::Button::new("Remove Component"))
                .clicked()
            {
                self.remove_component(entity_id);
            }
        });

        if matches!(kind, TypeKind::Struct { .. }) {
            ui.separator();
            ui.label("Patch (single field):");
            self.ui_patch_form(ui, &kind);
            if ui
                .add_enabled(caps.can_patch, egui::Button::new("Patch Component"))
                .clicked()
            {
                self.patch_component(entity_id);
            }
        }
    }

    fn ui_component_form(&mut self, ui: &mut egui::Ui, kind: &TypeKind) {
        match kind {
            TypeKind::Unit => {
                ui.label("<unit>");
            }
            TypeKind::Primitive { prim } => {
                let scalar = scalar_type_for_primitive(*prim);
                let entry = self
                    .component_fields
                    .entry("value".to_string())
                    .or_insert_with(|| FieldState {
                        scalar_type: scalar.clone(),
                        ..Default::default()
                    });
                entry.scalar_type = scalar;
                ui_field_editor(ui, "Value", entry);
            }
            TypeKind::Struct { fields } => {
                if fields.is_empty() {
                    ui.label("<empty struct>");
                    return;
                }
                for f in fields {
                    let entry = self
                        .component_fields
                        .entry(f.name.clone())
                        .or_insert_with(|| FieldState {
                            scalar_type: scalar_type_for_type_name(&f.type_name),
                            ..Default::default()
                        });
                    ui_field_editor(ui, &f.name, entry);
                }
            }
            TypeKind::Tuple { items } => {
                if items.is_empty() {
                    ui.label("<empty tuple>");
                    return;
                }
                for it in items {
                    let name = it.index.to_string();
                    let entry = self
                        .component_fields
                        .entry(name.clone())
                        .or_insert_with(|| FieldState {
                            scalar_type: scalar_type_for_type_name(&it.type_name),
                            ..Default::default()
                        });
                    ui_field_editor(ui, &format!("[{name}]"), entry);
                }
            }
            TypeKind::Enum { .. } => {
                let entry = self
                    .component_fields
                    .entry("value".to_string())
                    .or_insert_with(|| FieldState {
                        scalar_type: ScalarType::Value,
                        ..Default::default()
                    });
                entry.scalar_type = ScalarType::Value;
                ui_field_editor(ui, "Value", entry);
            }
            TypeKind::Other { kind } => {
                ui.label(format!("Unsupported editor for type kind: {kind}"));
                ui.label("Use the RPC console or implement a custom editor.");
            }
        }
    }

    fn ui_patch_form(&mut self, ui: &mut egui::Ui, kind: &TypeKind) {
        let TypeKind::Struct { fields } = kind else {
            return;
        };
        if fields.is_empty() {
            ui.label("<no fields>");
            return;
        }

        if self.patch_field_selected.is_empty() {
            self.patch_field_selected = fields[0].name.clone();
        }

        ui.horizontal(|ui| {
            ui.label("Field:");
            egui::ComboBox::from_id_salt("patch_field")
                .selected_text(&self.patch_field_selected)
                .show_ui(ui, |ui| {
                    for f in fields {
                        ui.selectable_value(
                            &mut self.patch_field_selected,
                            f.name.clone(),
                            &f.name,
                        );
                    }
                });
        });

        let field_type = fields
            .iter()
            .find(|f| f.name == self.patch_field_selected)
            .map(|f| f.type_name.as_str())
            .unwrap_or("json");
        let (inner_type, is_optional) = Self::unwrap_option_type_name(field_type);
        self.patch_field_value.scalar_type = scalar_type_for_type_name(inner_type);
        let scalar = self.patch_field_value.scalar_type.clone();
        Self::seed_field_state_defaults(
            &self.types,
            &mut self.patch_field_value,
            scalar,
            Some(inner_type),
            is_optional,
            false,
        );

        ui_field_editor(ui, "Value", &mut self.patch_field_value);
    }

    fn ui_type_kind(&self, ui: &mut egui::Ui, kind: &TypeKind) {
        match kind {
            TypeKind::Unit => {
                ui.label("unit");
            }
            TypeKind::Primitive { prim } => {
                ui.label(format!("{prim:?}"));
            }
            TypeKind::Struct { fields } => {
                if fields.is_empty() {
                    ui.label("<empty>");
                    return;
                }
                for f in fields {
                    ui.label(format!("{}: {}", f.name, f.type_name));
                }
            }
            TypeKind::Tuple { items } => {
                if items.is_empty() {
                    ui.label("<empty>");
                    return;
                }
                for it in items {
                    ui.label(format!("{}: {}", it.index, it.type_name));
                }
            }
            TypeKind::Enum { variants, repr } => {
                ui.label(format!("enum ({repr:?})"));
                if variants.is_empty() {
                    ui.label("<no variants>");
                    return;
                }
                for v in variants {
                    match &v.payload_type_name {
                        Some(payload) => ui.label(format!("{}: {}({})", v.index, v.name, payload)),
                        None => ui.label(format!("{}: {}", v.index, v.name)),
                    };
                }
            }
            TypeKind::Other { kind } => {
                ui.label(kind);
            }
        }
    }

    fn maybe_auto_refresh(&mut self, ctx: &egui::Context) {
        if !self.auto_refresh {
            return;
        }

        let now = Instant::now();
        let should_refresh = self
            .last_refresh
            .map(|t| now.duration_since(t) >= self.refresh_interval)
            .unwrap_or(true);

        if should_refresh {
            self.request_refresh();
            self.last_refresh = Some(now);
        }

        ctx.request_repaint_after(self.refresh_interval);
    }

    fn request_refresh(&mut self) {
        if self.refresh_in_flight {
            return;
        }

        let (host, port) = match self.connection_params() {
            Some(v) => v,
            None => return,
        };

        self.refresh_in_flight = true;
        // Keep the scene tree stable across tab switches.
        // Auto-refresh can run while another tab is active; if we send empty visibility/expansion
        // here, the worker will return a roots-only snapshot and we'd clobber `self.entities`.
        let mut visible = self.visible_entities.clone();
        if let Some(sel) = self.selected_entity
            && !visible.contains(&sel)
        {
            visible.push(sel);
        }
        let expanded = self.expanded_entities.clone();
        let _ = self.worker_tx.send(WorkerRequest::RefreshAll {
            host,
            port,
            visible_entities: visible,
            expanded_entities: expanded,
            selected_entity: self.selected_entity,
        });
    }

    fn spawn_empty(&mut self) {
        let (host, port) = match self.connection_params() {
            Some(v) => v,
            None => return,
        };
        let _ = self
            .worker_tx
            .send(WorkerRequest::SpawnEmpty { host, port });
    }

    fn spawn_entity(&mut self, parent: Option<u64>) {
        let (host, port) = match self.connection_params() {
            Some(v) => v,
            None => return,
        };

        let name = Some(match parent {
            Some(parent_id) => format!("Child Of {parent_id}"),
            None => "New Entity".to_string(),
        });

        let _ = self.worker_tx.send(WorkerRequest::SpawnEntity {
            host,
            port,
            parent,
            name,
        });
    }

    fn select_entity(&mut self, entity_id: u64) {
        self.selected_entity = Some(entity_id);
        self.selected_component = None;
    }

    fn select_component(&mut self, entity_id: u64, type_name: String) {
        self.selected_entity = Some(entity_id);
        self.selected_component = Some(type_name.clone());
        self.component_type_selected = type_name;
        self.rebuild_component_form(entity_id);
    }

    fn despawn_entity(&mut self, entity: u64) {
        let (host, port) = match self.connection_params() {
            Some(v) => v,
            None => return,
        };
        if self.selected_entity == Some(entity) {
            self.selected_entity = None;
        }
        let _ = self
            .worker_tx
            .send(WorkerRequest::Despawn { host, port, entity });
    }

    fn set_component(&mut self, entity: u64) {
        let (host, port) = match self.connection_params() {
            Some(v) => v,
            None => return,
        };

        self.component_form_error.clear();
        let type_name = self.component_type_selected.clone();
        if type_name.is_empty() {
            self.component_form_error = "Select a component type".to_string();
            return;
        }

        let kind = match self.types.iter().find(|t| t.type_name == type_name) {
            Some(t) => t.kind.clone(),
            None => {
                self.component_form_error = "Unknown component type".to_string();
                return;
            }
        };

        let json = match build_value_from_form(&kind, &self.component_fields) {
            Ok(v) => v,
            Err(e) => {
                self.component_form_error = e;
                return;
            }
        };

        let _ = self.worker_tx.send(WorkerRequest::SetComponent {
            host,
            port,
            entity,
            type_name,
            json,
        });
    }

    fn remove_component(&mut self, entity: u64) {
        let (host, port) = match self.connection_params() {
            Some(v) => v,
            None => return,
        };

        let type_name = self.component_type_selected.clone();
        if type_name.is_empty() {
            self.component_form_error = "Select a component type".to_string();
            return;
        }
        let _ = self.worker_tx.send(WorkerRequest::RemoveComponent {
            host,
            port,
            entity,
            type_name,
        });
    }

    fn patch_component(&mut self, entity: u64) {
        let (host, port) = match self.connection_params() {
            Some(v) => v,
            None => return,
        };

        self.component_form_error.clear();
        let type_name = self.component_type_selected.clone();
        if type_name.is_empty() {
            self.component_form_error = "Select a component type".to_string();
            return;
        }
        if self.patch_field_selected.is_empty() {
            self.component_form_error = "Select a field to patch".to_string();
            return;
        }

        let patch_value = match field_state_to_value(&self.patch_field_value) {
            Ok(v) => v,
            Err(e) => {
                self.component_form_error = e;
                return;
            }
        };

        let field_id = match self
            .types
            .iter()
            .find(|t| t.type_name == type_name)
            .map(|t| &t.kind)
        {
            Some(TypeKind::Struct { fields }) => fields
                .iter()
                .find(|f| f.name == self.patch_field_selected)
                .map(|f| f.field_id),
            _ => None,
        };

        let Some(field_id) = field_id else {
            self.component_form_error = "Unknown field".to_string();
            return;
        };

        let patch = Value::Map(vec![(Value::U64(field_id), patch_value)]);

        let _ = self.worker_tx.send(WorkerRequest::PatchComponent {
            host,
            port,
            entity,
            type_name,
            patch,
        });
    }

    fn invoke_rpc(&mut self) {
        let (host, port) = match self.connection_params() {
            Some(v) => v,
            None => return,
        };

        self.rpc_form_error.clear();

        let selected_args_kind = self
            .procs
            .iter()
            .find(|p| p.name == self.rpc_proc_input)
            .map(|p| p.args.clone());

        let args = match selected_args_kind {
            Some(
                kind @ (TypeKind::Unit
                | TypeKind::Primitive { .. }
                | TypeKind::Struct { .. }
                | TypeKind::Tuple { .. }
                | TypeKind::Enum { .. }),
            ) if !self.rpc_use_raw_value => {
                match build_value_from_form(&kind, &self.rpc_form_fields) {
                    Ok(v) => v,
                    Err(e) => {
                        self.rpc_form_error = e;
                        return;
                    }
                }
            }
            _ => {
                let mut state = self.rpc_args_value.clone();
                match state.try_build_value() {
                    Ok(v) => v,
                    Err(e) => {
                        self.rpc_form_error = e;
                        return;
                    }
                }
            }
        };

        let _ = self.worker_tx.send(WorkerRequest::Invoke {
            host,
            port,
            proc: self.rpc_proc_input.clone(),
            args,
        });
    }

    fn rebuild_rpc_form(&mut self, reset_values: bool) {
        self.rpc_form_error.clear();

        let Some(info) = self.procs.iter().find(|p| p.name == self.rpc_proc_input) else {
            if reset_values {
                self.rpc_form_fields.clear();
            }
            return;
        };

        if reset_values {
            let dv = Self::default_value_for_kind(&self.types, &info.args);
            self.rpc_args_value = ValueEditorState::from_value(&dv);
        }

        match &info.args {
            TypeKind::Other { .. } => {
                self.rpc_use_raw_value = true;
                if reset_values {
                    self.rpc_form_fields.clear();
                }
            }
            TypeKind::Unit => {
                if reset_values {
                    self.rpc_form_fields.clear();
                    self.rpc_use_raw_value = false;
                }
            }
            TypeKind::Primitive { prim } => {
                if reset_values {
                    self.rpc_form_fields.clear();
                    self.rpc_use_raw_value = false;
                }

                let scalar = scalar_type_for_primitive(*prim);
                self.rpc_form_fields.retain(|k, _| k == "value");

                let entry = self
                    .rpc_form_fields
                    .entry("value".to_string())
                    .or_insert_with(|| FieldState {
                        scalar_type: scalar.clone(),
                        ..Default::default()
                    });
                entry.scalar_type = scalar;
                let scalar = entry.scalar_type.clone();
                if matches!(scalar, ScalarType::Value)
                    && (reset_values || matches!(entry.value_state.kind, ValueKind::Null))
                {
                    let dv = Self::default_value_for_primitive(*prim);
                    entry.value_state = ValueEditorState::from_value(&dv);
                }
                Self::seed_field_state_defaults(
                    &self.types,
                    entry,
                    scalar,
                    None,
                    false,
                    reset_values,
                );
            }
            TypeKind::Enum { .. } => {
                if reset_values {
                    self.rpc_form_fields.clear();
                    self.rpc_use_raw_value = false;
                }

                self.rpc_form_fields.retain(|k, _| k == "value");
                let entry = self
                    .rpc_form_fields
                    .entry("value".to_string())
                    .or_insert_with(|| FieldState {
                        scalar_type: ScalarType::Value,
                        ..Default::default()
                    });
                entry.scalar_type = ScalarType::Value;

                let should_seed = reset_values || matches!(entry.value_state.kind, ValueKind::Null);
                if should_seed {
                    let dv = Self::default_value_for_kind(&self.types, &info.args);
                    entry.value_state = ValueEditorState::from_value(&dv);
                }
            }
            TypeKind::Struct { fields } => {
                if reset_values {
                    self.rpc_form_fields.clear();
                    self.rpc_use_raw_value = false;
                }

                let mut expected: std::collections::BTreeMap<String, (ScalarType, String, bool)> =
                    std::collections::BTreeMap::new();
                for f in fields {
                    let (inner, is_opt) = Self::unwrap_option_type_name(&f.type_name);
                    expected.insert(
                        f.name.clone(),
                        (scalar_type_for_type_name(inner), inner.to_string(), is_opt),
                    );
                }

                self.rpc_form_fields.retain(|k, _| expected.contains_key(k));

                for (name, (scalar, type_name, is_opt)) in expected {
                    let entry = self
                        .rpc_form_fields
                        .entry(name)
                        .or_insert_with(|| FieldState {
                            scalar_type: scalar.clone(),
                            ..Default::default()
                        });
                    entry.scalar_type = scalar;
                    let scalar = entry.scalar_type.clone();
                    Self::seed_field_state_defaults(
                        &self.types,
                        entry,
                        scalar,
                        Some(&type_name),
                        is_opt,
                        reset_values,
                    );
                }
            }
            TypeKind::Tuple { items } => {
                if reset_values {
                    self.rpc_form_fields.clear();
                    self.rpc_use_raw_value = false;
                }

                let mut expected: std::collections::BTreeMap<String, (ScalarType, String, bool)> =
                    std::collections::BTreeMap::new();
                for it in items {
                    let (inner, is_opt) = Self::unwrap_option_type_name(&it.type_name);
                    expected.insert(
                        it.index.to_string(),
                        (scalar_type_for_type_name(inner), inner.to_string(), is_opt),
                    );
                }

                self.rpc_form_fields.retain(|k, _| expected.contains_key(k));

                for (name, (scalar, type_name, is_opt)) in expected {
                    let entry = self
                        .rpc_form_fields
                        .entry(name)
                        .or_insert_with(|| FieldState {
                            scalar_type: scalar.clone(),
                            ..Default::default()
                        });
                    entry.scalar_type = scalar;
                    let scalar = entry.scalar_type.clone();
                    Self::seed_field_state_defaults(
                        &self.types,
                        entry,
                        scalar,
                        Some(&type_name),
                        is_opt,
                        reset_values,
                    );
                }
            }
        }
    }

    fn unwrap_option_type_name(type_name: &str) -> (&str, bool) {
        // Quick parse for the common `Option<T>` case in schema type names.
        // Keeps defaults/seed behavior aligned with "optional" server args.
        let s = type_name.trim();
        let s2 = s.trim_start_matches('&').trim();
        let s2 = s2.strip_prefix("mut ").unwrap_or(s2).trim();

        let base = s2.split('<').next().unwrap_or(s2).trim();
        let base = base.rsplit("::").next().unwrap_or(base).trim();
        if base != "Option" {
            return (type_name, false);
        }

        let start = match s2.find('<') {
            Some(i) => i,
            None => return (type_name, false),
        };
        let end = match s2.rfind('>') {
            Some(i) if i > start => i,
            _ => return (type_name, false),
        };
        let inner = s2[start + 1..end].trim();
        if inner.is_empty() {
            (type_name, false)
        } else {
            (inner, true)
        }
    }

    fn rebuild_component_form(&mut self, entity_id: u64) {
        self.component_form_error.clear();
        self.component_fields.clear();

        let type_name = self.component_type_selected.clone();
        if type_name.is_empty() {
            return;
        }

        let kind = match self.types.iter().find(|t| t.type_name == type_name) {
            Some(t) => t.kind.clone(),
            None => return,
        };

        let existing_value = self
            .entities
            .iter()
            .find(|e| e.entity == entity_id)
            .and_then(|e| e.components.iter().find(|c| c.type_name == type_name))
            .and_then(|c| c.value.as_ref());

        seed_form_from_existing(
            &self.types,
            &kind,
            existing_value,
            &mut self.component_fields,
        );

        if let TypeKind::Struct { fields } = &kind
            && self.patch_field_selected.is_empty()
            && let Some(f) = fields.first()
        {
            self.patch_field_selected = f.name.clone();
        }
    }

    fn connection_params(&mut self) -> Option<(String, u16)> {
        let port: u16 = match self.port_input.trim().parse() {
            Ok(p) => p,
            Err(_) => {
                self.status = "Invalid port".to_string();
                return None;
            }
        };

        Some((self.host.trim().to_string(), port))
    }

    fn drain_worker_responses(&mut self) {
        while let Ok(msg) = self.worker_rx.try_recv() {
            match msg {
                WorkerResponse::Status(s) => {
                    self.status = s;
                }
                WorkerResponse::Trace(t) => {
                    self.rpc_traces.push(t);
                    if self.rpc_traces.len() > Self::TRACE_MAX {
                        let drop_n = self.rpc_traces.len() - Self::TRACE_MAX;
                        self.rpc_traces.drain(0..drop_n);
                    }
                }
                WorkerResponse::Refreshed {
                    entities,
                    types,
                    procs,
                } => {
                    self.merge_entities_by_revision(entities);
                    self.types = types;
                    self.procs = procs;
                    self.refresh_in_flight = false;

                    if !self.component_type_selected.is_empty()
                        && !self
                            .types
                            .iter()
                            .any(|t| t.type_name == self.component_type_selected)
                    {
                        self.component_type_selected.clear();
                        self.component_fields.clear();
                    }

                    if !self.rpc_proc_input.is_empty()
                        && !self.procs.iter().any(|p| p.name == self.rpc_proc_input)
                    {
                        self.rpc_proc_input = self
                            .procs
                            .first()
                            .map(|p| p.name.clone())
                            .unwrap_or_default();
                    }

                    if !self.rpc_proc_input.is_empty() {
                        self.rebuild_rpc_form(false);
                    }
                }
                WorkerResponse::RefreshDone => {
                    self.refresh_in_flight = false;
                }
                WorkerResponse::Spawned { entity, parent } => {
                    self.selected_entity = Some(entity);
                    self.selected_component = None;
                    self.visible_entities.push(entity);
                    self.visible_entities.sort_unstable();
                    self.visible_entities.dedup();
                    if let Some(parent) = parent
                        && !self.expanded_entities.contains(&parent)
                    {
                        self.expanded_entities.push(parent);
                    }
                    self.request_refresh();
                }
                WorkerResponse::Invoked(v) => {
                    self.rpc_result_value = Some(v);
                }
            }
        }
    }
}
