use awrk_datex::value::Value;
use awrk_macros::Type;

#[Type]
#[derive(Debug, Clone)]
pub struct EntityInfo {
    pub entity: u64,
    pub revision: u64,
    pub components: Vec<ComponentInfo>,
}

#[Type]
#[derive(Debug, Clone)]
pub struct EntityMeta {
    pub entity: u64,
    pub revision: u64,
    pub parent: Option<u64>,
}

#[Type]
#[derive(Debug, Clone)]
pub struct ComponentInfo {
    pub type_name: String,
    pub value: Option<Value>,
}

#[Type]
#[derive(Debug, Clone)]
pub struct TypeInfo {
    pub type_name: String,
    pub kind: TypeKind,
}

#[Type]
#[derive(Debug, Clone, Copy, Default)]
pub struct TypeCaps {
    pub is_schema_root: bool,
    pub is_component: bool,
    pub can_read: bool,
    pub can_write: bool,
    pub can_patch: bool,
    pub can_remove: bool,
}

#[Type]
#[derive(Debug, Clone)]
pub struct TypeCapsInfo {
    pub type_name: String,
    pub caps: TypeCaps,
}

#[Type]
#[derive(Debug, Clone)]
pub struct ListTypesResult {
    pub types: Vec<TypeCapsInfo>,
}

#[Type]
#[derive(Debug, Clone)]
pub struct ProcInfo {
    pub name: String,
    pub args: TypeKind,
    pub result: TypeKind,
}

#[Type]
#[derive(Debug, Clone)]
pub enum TypeKind {
    Unit,
    Struct(Vec<FieldInfo>),
    Tuple(Vec<TupleItemInfo>),
    Other(String),
}

#[Type]
#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub type_name: String,
}

#[Type]
#[derive(Debug, Clone)]
pub struct TupleItemInfo {
    pub index: u32,
    pub type_name: String,
}

#[Type]
#[derive(Debug, Clone)]
pub struct SpawnArgs {
    pub components: Vec<ComponentInfo>,
}

#[Type]
#[derive(Debug, Clone)]
pub struct SpawnResult {
    pub entity: u64,
}

#[Type]
#[derive(Debug, Clone)]
pub struct DespawnArgs {
    pub entity: u64,
}

#[Type]
#[derive(Debug, Clone)]
pub struct RemoveComponentArgs {
    pub entity: u64,
    pub type_name: String,
}

#[Type]
#[derive(Debug, Clone)]
pub struct RemoveComponentResult {
    pub removed: bool,
}

#[Type]
#[derive(Debug, Clone)]
pub struct SetComponentArgs {
    pub entity: u64,
    pub type_name: String,
    pub value: Value,
}

#[Type]
#[derive(Debug, Clone)]
pub struct PatchComponentArgs {
    pub entity: u64,
    pub type_name: String,
    pub patch: Value,
}

#[Type]
#[derive(Debug, Clone)]
pub struct ListEntitiesResult {
    pub now: u64,
    pub entities: Vec<EntityMeta>,
}

#[Type]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Upserted,
    Despawned,
}

#[Type]
#[derive(Debug, Clone)]
pub struct ChangeEvent {
    pub seq: u64,
    pub entity: u64,
    pub revision: u64,
    pub parent: Option<u64>,
    pub kind: ChangeKind,
}

#[Type]
#[derive(Debug, Clone)]
pub struct PollChangesArgs {
    pub since: u64,
    pub limit: Option<u32>,
}

#[Type]
#[derive(Debug, Clone)]
pub struct PollChangesResult {
    pub now: u64,
    pub needs_resync: bool,
    pub cursor: u64,
    pub has_more: bool,
    pub events: Vec<ChangeEvent>,
}

#[Type]
#[derive(Debug, Clone)]
pub struct ListProceduresResult {
    pub procs: Vec<ProcInfo>,
}

#[Type]
#[derive(Debug, Clone)]
pub struct GetEntitiesArgs {
    pub entities: Vec<u64>,
}

#[Type]
#[derive(Debug, Clone)]
pub struct GetEntitiesResult {
    pub entities: Vec<EntityInfo>,
}

#[Type]
#[derive(Debug, Clone, Default)]
pub struct QueryEntitiesArgs {
    pub all_of: Vec<String>,
    pub any_of: Vec<String>,
    pub none_of: Vec<String>,
    pub after: Option<u64>,
    pub limit: Option<u32>,
}

#[Type]
#[derive(Debug, Clone)]
pub struct QueryEntitiesResult {
    pub entities: Vec<EntityMeta>,
    pub has_more: bool,
    pub next_after: Option<u64>,
}

#[Type]
#[derive(Debug, Clone)]
pub struct GetComponentArgs {
    pub entities: Vec<u64>,
    pub type_name: String,
}

#[Type]
#[derive(Debug, Clone)]
pub struct GetComponentResult {
    pub values: Vec<Option<Value>>,
}
