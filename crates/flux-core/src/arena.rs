use crate::{OutputId, WindowId, WorkspaceId};
use std::marker::PhantomData;

/// A typed generational arena key. Identity values are local to one arena
/// instance; an untrusted serialized identity never confers object authority.
/// Implementations must preserve both 32-bit parts exactly: `from_parts(i,g)`
/// must report the same `index()` and `generation()`.
pub trait ArenaId: Copy + Eq {
    fn from_parts(index: u32, generation: u32) -> Self;
    fn index(self) -> u32;
    fn generation(self) -> u32;
}
macro_rules! key {($($name:ty),*)=>{$(impl ArenaId for $name {
    fn from_parts(index:u32,generation:u32)->Self {Self::new(index,generation)}
    fn index(self)->u32 {self.index()}
    fn generation(self)->u32 {self.generation()}
})*}}
key!(WindowId, WorkspaceId, OutputId);
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ArenaCapacityError {
    #[error("arena index capacity exhausted")]
    IndexExhausted,
    #[error("arena slot allocation failed")]
    Allocation,
}
pub struct ArenaInsertError<T> {
    pub value: T,
    pub cause: ArenaCapacityError,
}
impl<T> std::fmt::Debug for ArenaInsertError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArenaInsertError")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl<T> std::fmt::Display for ArenaInsertError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}
impl<T> std::error::Error for ArenaInsertError<T> {}
struct Slot<T> {
    generation: u32,
    value: Option<T>,
    next_free: Option<u32>,
}
/// Owned reusable slots with checked generation advancement. Removing generation
/// u32::MAX permanently retires that slot; generations never wrap or alias stale
/// identities. A newly constructed arena is a distinct identity namespace.
pub struct Arena<T, I: ArenaId = WindowId> {
    slots: Vec<Slot<T>>,
    free: Option<u32>,
    len: usize,
    identity: PhantomData<I>,
}
impl<T, I: ArenaId> Default for Arena<T, I> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T, I: ArenaId> Arena<T, I> {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: None,
            len: 0,
            identity: PhantomData,
        }
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn contains(&self, id: I) -> bool {
        self.get(id).is_some()
    }
    /// Preserves ownership of the value if indexing or vector allocation fails.
    pub fn insert(&mut self, value: T) -> Result<I, ArenaInsertError<T>> {
        if let Some(index) = self.free {
            let slot = &mut self.slots[index as usize];
            self.free = slot.next_free.take();
            slot.value = Some(value);
            self.len += 1;
            return Ok(I::from_parts(index, slot.generation));
        }
        let Ok(index) = u32::try_from(self.slots.len()) else {
            return Err(ArenaInsertError {
                value,
                cause: ArenaCapacityError::IndexExhausted,
            });
        };
        if self.slots.try_reserve(1).is_err() {
            return Err(ArenaInsertError {
                value,
                cause: ArenaCapacityError::Allocation,
            });
        }
        self.slots.push(Slot {
            generation: 0,
            value: Some(value),
            next_free: None,
        });
        self.len += 1;
        Ok(I::from_parts(index, 0))
    }
    pub fn get(&self, id: I) -> Option<&T> {
        let slot = self.slots.get(id.index() as usize)?;
        if slot.generation == id.generation() {
            slot.value.as_ref()
        } else {
            None
        }
    }
    pub fn get_mut(&mut self, id: I) -> Option<&mut T> {
        let slot = self.slots.get_mut(id.index() as usize)?;
        if slot.generation == id.generation() {
            slot.value.as_mut()
        } else {
            None
        }
    }
    pub fn remove(&mut self, id: I) -> Option<T> {
        let slot = self.slots.get_mut(id.index() as usize)?;
        if slot.generation != id.generation() {
            return None;
        }
        let value = slot.value.take()?;
        self.len -= 1;
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            slot.next_free = self.free;
            self.free = Some(id.index());
        }
        Some(value)
    }
    pub fn iter(&self) -> impl Iterator<Item = (I, &T)> {
        self.slots.iter().enumerate().filter_map(|(i, slot)| {
            slot.value
                .as_ref()
                .map(|v| (I::from_parts(i as u32, slot.generation), v))
        })
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (I, &mut T)> {
        self.slots.iter_mut().enumerate().filter_map(|(i, slot)| {
            slot.value
                .as_mut()
                .map(|v| (I::from_parts(i as u32, slot.generation), v))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removed_identity_never_resolves_or_removes_replacement() {
        let mut arena = Arena::<String>::new();
        let old = arena.insert("first".into()).unwrap();
        assert_eq!(arena.remove(old).as_deref(), Some("first"));
        let new = arena.insert("second".into()).unwrap();
        assert_eq!(old.index(), new.index());
        assert_ne!(old.generation(), new.generation());
        assert!(arena.get(old).is_none());
        assert!(arena.get_mut(old).is_none());
        assert!(arena.remove(old).is_none());
        assert_eq!(arena.get(new).map(String::as_str), Some("second"));
        assert_eq!(arena.len(), 1);
    }
    #[test]
    fn generation_exhaustion_retires_slot_without_wrapping() {
        let mut arena = Arena::<u32>::new();
        let first = arena.insert(1).unwrap();
        // Inject the last live generation without billions of operations.
        arena.slots[first.index() as usize].generation = u32::MAX - 1;
        let penultimate = WindowId::new(first.index(), u32::MAX - 1);
        assert_eq!(arena.remove(penultimate), Some(1));
        let final_id = arena.insert(2).unwrap();
        assert_eq!(final_id.generation(), u32::MAX);
        assert_eq!(arena.remove(final_id), Some(2));
        assert!(arena.remove(final_id).is_none());
        let fresh = arena.insert(3).unwrap();
        assert_ne!(fresh.index(), final_id.index());
        assert_eq!(fresh.generation(), 0);
        for stale in [first, penultimate, final_id] {
            assert!(arena.get(stale).is_none());
        }
    }
    #[test]
    fn free_list_iteration_and_mutation_keep_live_count_consistent() {
        let mut arena = Arena::<usize, WorkspaceId>::new();
        let ids = (0..100)
            .map(|n| arena.insert(n).unwrap())
            .collect::<Vec<_>>();
        for id in ids.iter().step_by(2) {
            assert!(arena.remove(*id).is_some());
        }
        for _ in 0..50 {
            arena.insert(100).unwrap();
        }
        assert_eq!(arena.len(), 100);
        for (_, value) in arena.iter_mut() {
            *value += 1;
        }
        assert_eq!(arena.iter().count(), 100);
        let keys = arena.iter().map(|(id, _)| id).collect::<Vec<_>>();
        for id in keys {
            assert!(arena.remove(id).is_some());
        }
        assert!(arena.is_empty());
        assert_eq!(arena.iter().count(), 0);
        assert!(arena.get(WorkspaceId::new(u32::MAX, 0)).is_none());
    }
}
