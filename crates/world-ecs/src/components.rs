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
