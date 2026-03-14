use crate::Parent;
use crate::rpc::{ChangeEvent, ChangeKind, PollChangesResult};
use std::collections::{HashMap, VecDeque};

const CHANGE_LOG_MAX: usize = 1_000_000;

fn entity_parent_bits(world: &hecs::World, entity: hecs::Entity) -> Option<u64> {
    world.get::<&Parent>(entity).ok().map(|p| p.parent)
}

fn push_change_event(change_log: &mut VecDeque<ChangeEvent>, event: ChangeEvent) {
    if let Some(last) = change_log.back_mut() {
        if last.entity == event.entity {
            *last = event;
            return;
        }
    }

    change_log.push_back(event);
    while change_log.len() > CHANGE_LOG_MAX {
        change_log.pop_front();
    }
}

#[derive(Debug)]
pub struct ChangeLog {
    next_change_seq: u64,
    entity_revisions: HashMap<hecs::Entity, u64>,
    change_log: VecDeque<ChangeEvent>,
}

impl Default for ChangeLog {
    fn default() -> Self {
        Self {
            next_change_seq: 1,
            entity_revisions: HashMap::new(),
            change_log: VecDeque::new(),
        }
    }
}

impl ChangeLog {
    pub fn now(&self) -> u64 {
        self.next_change_seq - 1
    }

    pub fn entity_revision(&self, entity: hecs::Entity) -> u64 {
        self.entity_revisions.get(&entity).copied().unwrap_or(0)
    }

    pub fn remove_entity_revision(&mut self, entity: hecs::Entity) {
        self.entity_revisions.remove(&entity);
    }

    pub fn bump_entity_revision(&mut self, world: &hecs::World, entity: hecs::Entity) -> u64 {
        let parent = entity_parent_bits(world, entity);
        let rev = self.alloc_entity_revision(entity);

        push_change_event(
            &mut self.change_log,
            ChangeEvent {
                seq: rev,
                entity: entity.to_bits().get(),
                revision: rev,
                parent,
                kind: ChangeKind::Upserted,
            },
        );

        rev
    }

    pub fn bump_entity_revision_with_parent(
        &mut self,
        entity: hecs::Entity,
        parent: Option<u64>,
    ) -> u64 {
        let rev = self.alloc_entity_revision(entity);

        push_change_event(
            &mut self.change_log,
            ChangeEvent {
                seq: rev,
                entity: entity.to_bits().get(),
                revision: rev,
                parent,
                kind: ChangeKind::Upserted,
            },
        );

        rev
    }

    pub fn log_despawn(&mut self, entity_bits: u64) {
        let seq = self.alloc_change_seq();
        push_change_event(
            &mut self.change_log,
            ChangeEvent {
                seq,
                entity: entity_bits,
                revision: 0,
                parent: None,
                kind: ChangeKind::Despawned,
            },
        );
    }

    pub fn poll(&self, since: u64, limit: Option<u32>) -> Result<PollChangesResult, String> {
        let now = self.now();
        let max = limit.unwrap_or(2048) as usize;

        if since >= now {
            return Ok(PollChangesResult {
                now,
                needs_resync: false,
                cursor: now,
                has_more: false,
                events: Vec::new(),
            });
        }

        let Some(first) = self.change_log.front() else {
            return Ok(PollChangesResult {
                now,
                needs_resync: true,
                cursor: now,
                has_more: false,
                events: Vec::new(),
            });
        };

        if since < first.seq {
            return Ok(PollChangesResult {
                now,
                needs_resync: true,
                cursor: now,
                has_more: false,
                events: Vec::new(),
            });
        }

        let mut events = Vec::new();
        for ev in self.change_log.iter() {
            if ev.seq <= since {
                continue;
            }
            events.push(ev.clone());
            if events.len() >= max {
                break;
            }
        }

        let cursor = events.last().map(|e| e.seq).unwrap_or(now);
        let has_more = cursor < now;

        Ok(PollChangesResult {
            now,
            needs_resync: false,
            cursor,
            has_more,
            events,
        })
    }

    fn alloc_change_seq(&mut self) -> u64 {
        let out = self.next_change_seq;
        self.next_change_seq = self.next_change_seq.wrapping_add(1);
        out
    }

    fn alloc_entity_revision(&mut self, entity: hecs::Entity) -> u64 {
        let rev = self.alloc_change_seq();
        self.entity_revisions.insert(entity, rev);
        rev
    }
}
