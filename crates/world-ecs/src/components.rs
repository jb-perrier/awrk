#[derive(
    Debug,
    Clone,
    awrk_datex::Encode,
    awrk_datex::Decode,
    awrk_datex::Patch,
    awrk_schema_macros::Schema,
)]
pub struct Name(pub String);

#[derive(
    Debug,
    Clone,
    awrk_datex::Encode,
    awrk_datex::Decode,
    awrk_datex::Patch,
    awrk_schema_macros::Schema,
)]
pub struct Parent {
    pub parent: u64,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    awrk_datex::Encode,
    awrk_datex::Decode,
    awrk_datex::Patch,
    awrk_schema_macros::Schema,
)]
pub struct WorldId(pub u64);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    awrk_datex::Encode,
    awrk_datex::Decode,
    awrk_datex::Patch,
    awrk_schema_macros::Schema,
)]
pub struct RemoteRef {
    pub world_id: u64,
    pub entity: u64,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    awrk_datex::Encode,
    awrk_datex::Decode,
    awrk_datex::Patch,
    awrk_schema_macros::Schema,
)]
pub enum ProxyLifecycle {
    Creating,
    Live,
    Stale,
    Disconnected,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    awrk_datex::Encode,
    awrk_datex::Decode,
    awrk_datex::Patch,
    awrk_schema_macros::Schema,
)]
pub enum ProxyAuthorityKind {
    Remote,
    Local,
    ReadOnlyMirror,
    RequestDriven,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    awrk_datex::Encode,
    awrk_datex::Decode,
    awrk_datex::Patch,
    awrk_schema_macros::Schema,
)]
pub struct ProxyEntity {
    pub remote: RemoteRef,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    awrk_datex::Encode,
    awrk_datex::Decode,
    awrk_datex::Patch,
    awrk_schema_macros::Schema,
)]
pub struct ProxyState {
    pub last_remote_revision: u64,
    pub lifecycle: ProxyLifecycle,
    pub last_seen_at: u64,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    awrk_datex::Encode,
    awrk_datex::Decode,
    awrk_datex::Patch,
    awrk_schema_macros::Schema,
)]
pub struct ProxyAuthority {
    pub authority: ProxyAuthorityKind,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    awrk_datex::Encode,
    awrk_datex::Decode,
    awrk_datex::Patch,
    awrk_schema_macros::Schema,
)]
pub struct ProxySpawnRequest;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    awrk_datex::Encode,
    awrk_datex::Decode,
    awrk_datex::Patch,
    awrk_schema_macros::Schema,
)]
pub struct ProxySpawnError {
    pub message: String,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    awrk_datex::Encode,
    awrk_datex::Decode,
    awrk_datex::Patch,
    awrk_schema_macros::Schema,
)]
pub struct RemoteParentRef {
    pub remote: RemoteRef,
}
