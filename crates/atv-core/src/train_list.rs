//!Native001abae0 dynamic list selected by Arlington's optimized scene setup.
//!Movement leaves stable slots unchanged. Source removal uses a LIFO free list;
//!query001abde0 scans allocated slots in order, skipping removed entries.
use crate::Result;
#[derive(Debug, Clone, Default)]
pub struct DynamicList {
    slots: Vec<Option<usize>>,
    free: Vec<usize>,
    keys: std::collections::BTreeMap<usize, usize>,
}
impl DynamicList {
    pub fn register(&mut self, id: usize) -> Result<()> {
        if self.keys.contains_key(&id) {
            return Err("dynamic actor already registered".into());
        }
        let slot = if let Some(slot) = self.free.pop() {
            self.slots[slot] = Some(id);
            slot
        } else {
            let slot = self.slots.len();
            self.slots.push(Some(id));
            slot
        };
        self.keys.insert(id, slot);
        Ok(())
    }
    pub fn unregister(&mut self, id: usize) -> bool {
        let Some(slot) = self.keys.remove(&id) else {
            return false;
        };
        self.slots[slot] = None;
        self.free.push(slot);
        true
    }
    ///The source provider ignores query bounds and returns every active slot.
    ///Category/actor eligibility and geometry are later consumer stages.
    pub fn indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.slots.iter().filter_map(|slot| *slot)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_lifo_holes_reuse_slots_without_reordering_survivors() {
        let mut list = DynamicList::default();
        for id in 0..8 {
            list.register(id).unwrap();
        }
        for id in [2, 5, 1] {
            assert!(list.unregister(id));
        }
        assert_eq!(list.indices().collect::<Vec<_>>(), [0, 3, 4, 6, 7]);
        for id in [20, 21, 22, 23] {
            list.register(id).unwrap();
        }
        assert_eq!(
            list.indices().collect::<Vec<_>>(),
            [0, 20, 22, 3, 4, 21, 6, 7, 23]
        );
        assert!(list.register(20).is_err());
        assert!(!list.unregister(2));
        assert_eq!(
            list.indices().collect::<Vec<_>>(),
            [0, 20, 22, 3, 4, 21, 6, 7, 23]
        );
    }
    #[test]
    fn native_1024_slot_block_boundary_keeps_global_order() {
        let mut list = DynamicList::default();
        for id in 0..1025 {
            list.register(id).unwrap();
        }
        list.unregister(1023);
        list.unregister(1024);
        list.register(2000).unwrap();
        list.register(2001).unwrap();
        assert_eq!(
            list.indices().skip(1022).collect::<Vec<_>>(),
            [1022, 2001, 2000]
        );
    }
}
