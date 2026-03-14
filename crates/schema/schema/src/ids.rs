#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StringId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcId(pub u64);

const FNV_OFFSET_BASIS_64: u64 = 0xcbf29ce484222325;
const FNV_PRIME_64: u64 = 0x100000001b3;

pub fn string_id(value: &str) -> StringId {
    StringId(fnv1a_64(value.as_bytes()))
}

pub fn type_id(type_name: &str) -> TypeId {
    let mut h = FNV_OFFSET_BASIS_64;
    h = fnv1a_extend(h, b"::");
    h = fnv1a_extend(h, type_name.as_bytes());
    TypeId(h)
}

pub fn field_id(parent_type: TypeId, field_name: &str) -> FieldId {
    let mut h = FNV_OFFSET_BASIS_64;
    h = fnv1a_extend(h, b"::");
    h = fnv1a_extend(h, &parent_type.0.to_le_bytes());
    h = fnv1a_extend(h, b"::");
    h = fnv1a_extend(h, field_name.as_bytes());
    FieldId(h)
}

pub fn proc_id(proc_name: &str) -> ProcId {
    let mut h = FNV_OFFSET_BASIS_64;
    h = fnv1a_extend(h, b"::");
    h = fnv1a_extend(h, proc_name.as_bytes());
    ProcId(h)
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    fnv1a_extend(FNV_OFFSET_BASIS_64, bytes)
}

fn fnv1a_extend(mut hash: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(FNV_PRIME_64);
    }
    hash
}
