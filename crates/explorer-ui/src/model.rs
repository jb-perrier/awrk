use awrk_datex::codec::decode::DecodeConfig;
use awrk_datex::value::Value;
use awrk_datex::{Decode, Encode};
use awrk_world::rpc::EntityInfo;
use awrk_world_ecs::{Name, Parent};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct NodeInfo {
    pub id: u64,
    pub name: Option<String>,
    pub parent: Option<u64>,
    pub entity_index: usize,
}

#[derive(Clone, Debug, Default)]
pub struct TreeData {
    pub nodes: HashMap<u64, NodeInfo>,
    pub children: HashMap<u64, Vec<u64>>,
    pub roots: Vec<u64>,
}

pub fn build_tree(entities: &[EntityInfo]) -> TreeData {
    let parent_type_name = std::any::type_name::<Parent>();
    let name_type_name = std::any::type_name::<Name>();

    let mut nodes: HashMap<u64, NodeInfo> = HashMap::with_capacity(entities.len());
    for (entity_index, e) in entities.iter().enumerate() {
        let parent = find_component_value(e, parent_type_name)
            .and_then(|v| decode_typed_from_value::<Parent>(v).ok())
            .map(|p| p.parent);
        let name = find_component_value(e, name_type_name)
            .and_then(|v| decode_typed_from_value::<Name>(v).ok())
            .map(|n| n.0);
        nodes.insert(
            e.entity,
            NodeInfo {
                id: e.entity,
                name,
                parent,
                entity_index,
            },
        );
    }

    let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
    let mut roots: Vec<u64> = Vec::new();

    for node in nodes.values() {
        if let Some(p) = node.parent
            && p != node.id
            && nodes.contains_key(&p)
        {
            children.entry(p).or_default().push(node.id);
            continue;
        }
        roots.push(node.id);
    }

    for v in children.values_mut() {
        v.sort_unstable();
    }
    roots.sort_unstable();

    TreeData {
        nodes,
        children,
        roots,
    }
}

pub fn find_component_value<'a>(entity: &'a EntityInfo, type_name: &str) -> Option<&'a Value> {
    entity
        .components
        .iter()
        .find(|c| c.type_name == type_name)
        .and_then(|c| c.value.as_ref())
}

fn decode_typed_from_value<R>(value: &Value) -> Result<R, String>
where
    for<'de> R: Decode<'de>,
{
    let mut enc = awrk_datex::codec::encode::Encoder::default();
    value.wire_encode(&mut enc).map_err(|e| e.to_string())?;
    let buf = enc.into_inner();
    let value_ref = awrk_datex::codec::decode::decode_value_full(&buf, DecodeConfig::default())
        .map_err(|e| e.to_string())?;
    R::wire_decode(value_ref).map_err(|e| e.to_string())
}
