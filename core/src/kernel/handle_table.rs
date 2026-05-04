use log::{trace, warn};
use std::cell::Cell;
use std::fmt;

/// Result codes matching Horizon kernel conventions.
pub mod result {
    /// The operation completed successfully.
    pub const SUCCESS: u32 = 0;
    /// The requested service is not registered.
    pub const SERVICE_NOT_FOUND: u32 = 0x415;
    /// The requested method ID is not implemented by the service.
    pub const NOT_IMPLEMENTED: u32 = 0x1A01;
    /// The handle ID is invalid, freed, or the wrong type.
    pub const INVALID_HANDLE: u32 = 0xD401;
    /// The handle table is at maximum capacity with no free slots.
    pub const OUT_OF_HANDLES: u32 = 0xD402;
}

/// Types of kernel objects that can be stored in the handle table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelObject {
    Session(String), // service name
    Event,
    SharedMemory { size: usize },
    Process,
    Thread,
    Port,
}

impl fmt::Display for KernelObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KernelObject::Session(name) => write!(f, "Session({name})"),
            KernelObject::Event => write!(f, "Event"),
            KernelObject::SharedMemory { size } => write!(f, "SharedMemory({size})"),
            KernelObject::Process => write!(f, "Process"),
            KernelObject::Thread => write!(f, "Thread"),
            KernelObject::Port => write!(f, "Port"),
        }
    }
}

impl KernelObject {
    /// Returns a short discriminant name for logging.
    pub fn type_name(&self) -> &'static str {
        match self {
            KernelObject::Session(_) => "Session",
            KernelObject::Event => "Event",
            KernelObject::SharedMemory { .. } => "SharedMemory",
            KernelObject::Process => "Process",
            KernelObject::Thread => "Thread",
            KernelObject::Port => "Port",
        }
    }

    /// Returns true if `other` is the same variant (structural match, not value).
    pub fn same_type_as(&self, other: &KernelObject) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// An entry in the handle table.
struct HandleEntry {
    object: KernelObject,
    ref_count: Cell<u32>,
    handle_id: u32,
}

/// A type-safe, reference-counted registry for kernel objects.
///
/// Maps opaque 32-bit handle IDs to typed [`KernelObject`]s. `get_handle` validates
/// the expected type and rejects mismatches. `release_handle` decrements ref counts
/// and frees slots at zero. The table starts at 1024 entries, doubles on overflow,
/// and caps at 65536. All operations are O(1) via direct array indexing with a
/// free-list.
pub struct HandleTable {
    entries: Vec<Option<HandleEntry>>,
    free_list: Vec<u32>,
    next_slot: u32,
    capacity: usize,
    max_capacity: usize,
}

impl HandleTable {
    /// Creates a new handle table with 1024 initial capacity.
    pub fn new() -> Self {
        const INITIAL_CAPACITY: usize = 1024;
        let mut entries = Vec::with_capacity(INITIAL_CAPACITY);
        entries.resize_with(INITIAL_CAPACITY, || None);
        HandleTable {
            entries,
            free_list: Vec::new(),
            next_slot: 0,
            capacity: INITIAL_CAPACITY,
            max_capacity: 65536,
        }
    }

    /// Creates a new handle with the given object, returning its handle ID.
    ///
    /// On success the new handle has ref_count = 1.
    /// Returns `Err(result::OUT_OF_HANDLES)` if the table is at max capacity
    /// with no free slots.
    pub fn create_handle(&mut self, object: KernelObject) -> Result<u32, u32> {
        let slot = self.allocate_slot()?;
        let handle_id = slot;
        trace!(
            "HandleTable: create handle_id={}, type={}",
            handle_id,
            object.type_name()
        );
        self.entries[slot as usize] = Some(HandleEntry {
            object,
            ref_count: Cell::new(1),
            handle_id,
        });
        Ok(handle_id)
    }

    /// Borrows a kernel object by handle ID, validating its type.
    ///
    /// Increments the reference count. The caller must eventually call
    /// [`release_handle`] to decrement it.
    ///
    /// Returns `Err(result::INVALID_HANDLE)` if the handle ID is invalid,
    /// the slot is free, or the stored object does not match `expected_type`.
    pub fn get_handle(&self, id: u32, expected_type: &KernelObject) -> Result<&KernelObject, u32> {
        let entry = self
            .entries
            .get(id as usize)
            .and_then(|opt| opt.as_ref())
            .ok_or_else(|| {
                warn!("HandleTable::get_handle: invalid handle_id={}", id);
                result::INVALID_HANDLE
            })?;

        if !entry.object.same_type_as(expected_type) {
            warn!(
                "HandleTable::get_handle: type mismatch handle_id={}, expected={}, got={}",
                id,
                expected_type.type_name(),
                entry.object.type_name()
            );
            return Err(result::INVALID_HANDLE);
        }

        let old = entry.ref_count.get();
        entry.ref_count.set(old + 1);
        trace!(
            "HandleTable::get_handle id={} type={} ref_count={}->{}",
            id,
            entry.object.type_name(),
            old,
            old + 1
        );
        Ok(&entry.object)
    }

    /// Decrements the reference count for a handle.
    ///
    /// If the ref count reaches zero, the slot is freed and the handle ID
    /// is returned to the free-list for reuse.
    ///
    /// Returns `Err(result::INVALID_HANDLE)` if the handle ID is invalid or
    /// the slot is already free.
    pub fn release_handle(&mut self, id: u32) -> Result<(), u32> {
        let entry = self
            .entries
            .get_mut(id as usize)
            .and_then(|opt| opt.as_mut())
            .ok_or_else(|| {
                warn!("HandleTable::release_handle: invalid handle_id={}", id);
                result::INVALID_HANDLE
            })?;

        let old = entry.ref_count.get();
        if old == 0 {
            warn!(
                "HandleTable::release_handle: double-release handle_id={}",
                id
            );
            return Err(result::INVALID_HANDLE);
        }

        let new = old - 1;
        entry.ref_count.set(new);
        trace!(
            "HandleTable::release_handle id={} type={} ref_count={}->{}",
            id,
            entry.object.type_name(),
            old,
            new
        );

        if new == 0 {
            trace!("HandleTable: freeing slot handle_id={}", id);
            self.entries[id as usize] = None;
            self.free_list.push(id);
        }

        Ok(())
    }

    /// Number of occupied slots (non-None entries).
    pub fn len(&self) -> usize {
        self.entries.iter().filter(|e| e.is_some()).count()
    }

    /// Current total capacity of the table.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of active (non-None) handles.
    pub fn active_handles(&self) -> usize {
        self.len()
    }

    /// Number of free slots available.
    pub fn free_slots(&self) -> usize {
        self.capacity - self.len()
    }

    /// Panics if any slot has a non-zero reference count, indicating a leak.
    ///
    /// Call this in tests after releasing all handles to verify correctness.
    pub fn verify_no_leaks(&self) {
        for (idx, entry) in self.entries.iter().enumerate() {
            if let Some(e) = entry {
                let rc = e.ref_count.get();
                if rc != 0 {
                    panic!(
                        "LEAK: handle_id={} type={} has ref_count={}",
                        e.handle_id,
                        e.object.type_name(),
                        rc
                    );
                }
            }
        }
    }

    /// Returns a list of (handle_id, type_name, ref_count) for all active handles.
    pub fn dump_handles(&self) -> Vec<(u32, String, u32)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                entry.as_ref().map(|e| {
                    (
                        e.handle_id,
                        e.object.type_name().to_string(),
                        e.ref_count.get(),
                    )
                })
            })
            .collect()
    }

    // ── internal helpers ──

    /// Finds an available slot: prefers the free-list, then next_slot.
    /// Grows the table if needed. Returns the slot index.
    fn allocate_slot(&mut self) -> Result<u32, u32> {
        if let Some(recycled) = self.free_list.pop() {
            return Ok(recycled);
        }

        if (self.next_slot as usize) >= self.capacity {
            self.grow()?;
        }

        let slot = self.next_slot;
        self.next_slot += 1;
        Ok(slot)
    }

    /// Doubles capacity up to max_capacity (65536).
    fn grow(&mut self) -> Result<(), u32> {
        let new_capacity = (self.capacity * 2).min(self.max_capacity);
        if new_capacity <= self.capacity {
            warn!(
                "HandleTable::grow: at max capacity {}, cannot grow",
                self.max_capacity
            );
            return Err(result::OUT_OF_HANDLES);
        }

        trace!(
            "HandleTable::grow {} -> {}",
            self.capacity,
            new_capacity
        );

        self.entries.resize_with(new_capacity, || None);
        self.capacity = new_capacity;
        Ok(())
    }
}

impl Default for HandleTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── CreateHandle ──────────────────────────────────────────────

    #[test]
    fn create_single_handle() {
        let mut ht = HandleTable::new();
        let id = ht.create_handle(KernelObject::Event).unwrap();
        assert_eq!(id, 0);
        assert_eq!(ht.active_handles(), 1);
    }

    #[test]
    fn create_multiple_handles_sequential_ids() {
        let mut ht = HandleTable::new();
        for i in 0..10u32 {
            let id = ht.create_handle(KernelObject::Thread).unwrap();
            assert_eq!(id, i);
        }
        assert_eq!(ht.active_handles(), 10);
    }

    #[test]
    fn create_at_max_capacity_returns_out_of_handles() {
        let mut ht = HandleTable::new();
        // Fill to 65536
        for i in 0..65536u32 {
            let id = ht.create_handle(KernelObject::Event).unwrap();
            assert_eq!(id, i);
        }
        assert_eq!(ht.active_handles(), 65536);
        assert_eq!(ht.len(), 65536);
        // Now at max — no free slots
        let result = ht.create_handle(KernelObject::Event);
        assert_eq!(result, Err(result::OUT_OF_HANDLES));
    }

    // ── GetHandle ──────────────────────────────────────────────────

    #[test]
    fn get_handle_returns_object() {
        let mut ht = HandleTable::new();
        let id = ht.create_handle(KernelObject::Event).unwrap();
        let obj = ht.get_handle(id, &KernelObject::Event).unwrap();
        assert_eq!(*obj, KernelObject::Event);
    }

    #[test]
    fn get_handle_type_mismatch_returns_invalid_handle() {
        let mut ht = HandleTable::new();
        let id = ht.create_handle(KernelObject::Session("ns".into())).unwrap();
        let result = ht.get_handle(id, &KernelObject::Event);
        assert_eq!(result, Err(result::INVALID_HANDLE));
    }

    #[test]
    fn get_handle_invalid_id_returns_error() {
        let ht = HandleTable::new();
        let result = ht.get_handle(0, &KernelObject::Event);
        assert_eq!(result, Err(result::INVALID_HANDLE));
    }

    #[test]
    fn get_handle_u32_max_returns_error() {
        let ht = HandleTable::new();
        let result = ht.get_handle(u32::MAX, &KernelObject::Event);
        assert_eq!(result, Err(result::INVALID_HANDLE));
    }

    #[test]
    fn get_handle_freed_handle_returns_error() {
        let mut ht = HandleTable::new();
        let id = ht.create_handle(KernelObject::Event).unwrap();
        ht.release_handle(id).unwrap();
        let result = ht.get_handle(id, &KernelObject::Event);
        assert_eq!(result, Err(result::INVALID_HANDLE));
    }

    #[test]
    fn get_handle_increments_ref_count() {
        let mut ht = HandleTable::new();
        let id = ht.create_handle(KernelObject::Event).unwrap();
        // ref_count starts at 1; each get_handle bumps it
        ht.get_handle(id, &KernelObject::Event).unwrap(); // 2
        ht.get_handle(id, &KernelObject::Event).unwrap(); // 3
        // Release should need three calls
        ht.release_handle(id).unwrap(); // 2 -> still alive
        assert_eq!(ht.active_handles(), 1);
        ht.release_handle(id).unwrap(); // 1 -> still alive
        assert_eq!(ht.active_handles(), 1);
        ht.release_handle(id).unwrap(); // 0 -> freed
        assert_eq!(ht.active_handles(), 0);
    }

    // ── ReleaseHandle ──────────────────────────────────────────────

    #[test]
    fn release_handle_frees_at_zero() {
        let mut ht = HandleTable::new();
        let id = ht.create_handle(KernelObject::Event).unwrap();
        ht.release_handle(id).unwrap();
        assert_eq!(ht.active_handles(), 0);
    }

    #[test]
    fn release_handle_invalid_id_returns_error() {
        let mut ht = HandleTable::new();
        let result = ht.release_handle(0);
        assert_eq!(result, Err(result::INVALID_HANDLE));
    }

    #[test]
    fn release_handle_double_release_returns_error() {
        let mut ht = HandleTable::new();
        let id = ht.create_handle(KernelObject::Event).unwrap();
        ht.release_handle(id).unwrap();
        let result = ht.release_handle(id);
        assert_eq!(result, Err(result::INVALID_HANDLE));
    }

    // ── Handle ID reuse from free-list ─────────────────────────────

    #[test]
    fn handle_id_reuse_from_free_list() {
        let mut ht = HandleTable::new();
        let id0 = ht.create_handle(KernelObject::Event).unwrap();
        let id1 = ht.create_handle(KernelObject::Thread).unwrap();
        ht.release_handle(id0).unwrap();
        // Next allocation should reuse id0
        let reused = ht.create_handle(KernelObject::Process).unwrap();
        assert_eq!(reused, id0);
        // Next should be fresh (id2)
        let fresh = ht.create_handle(KernelObject::Port).unwrap();
        assert_eq!(fresh, id1 as u32 + 1);
    }

    // ── Growth ─────────────────────────────────────────────────────

    #[test]
    fn grow_at_1024_boundary() {
        let mut ht = HandleTable::new();
        // Fill exactly 1024
        for i in 0..1024u32 {
            let id = ht.create_handle(KernelObject::Thread).unwrap();
            assert_eq!(id, i);
        }
        assert_eq!(ht.capacity(), 1024);
        // Next create triggers grow → capacity doubles
        let id = ht.create_handle(KernelObject::Event).unwrap();
        assert_eq!(id, 1024);
        assert_eq!(ht.capacity(), 2048);
        assert_eq!(ht.active_handles(), 1025);
    }

    #[test]
    fn grow_sequence_1024_2048_4096() {
        let mut ht = HandleTable::new();
        for i in 0..4096u32 {
            ht.create_handle(KernelObject::Thread).unwrap();
        }
        // We went through grow at 1024→2048 and 2048→4096
        assert_eq!(ht.capacity(), 4096);
    }

    // ── verify_no_leaks ────────────────────────────────────────────

    #[test]
    fn verify_no_leaks_on_empty_table() {
        let ht = HandleTable::new();
        ht.verify_no_leaks(); // should not panic
    }

    #[test]
    fn verify_no_leaks_after_full_cycle() {
        let mut ht = HandleTable::new();
        for _ in 0..100u32 {
            let id = ht.create_handle(KernelObject::Event).unwrap();
            ht.release_handle(id).unwrap();
        }
        ht.verify_no_leaks();
    }

    #[test]
    #[should_panic(expected = "LEAK")]
    fn verify_no_leaks_detects_leaked_handle() {
        let mut ht = HandleTable::new();
        ht.create_handle(KernelObject::Event).unwrap();
        // Never released
        ht.verify_no_leaks();
    }

    #[test]
    #[should_panic(expected = "LEAK")]
    fn verify_no_leaks_detects_under_released_handle() {
        let mut ht = HandleTable::new();
        let id = ht.create_handle(KernelObject::Event).unwrap();
        ht.get_handle(id, &KernelObject::Event).unwrap(); // ref_count = 2
        ht.release_handle(id).unwrap(); // ref_count = 1 — leaked
        ht.verify_no_leaks();
    }

    // ── Stress test ────────────────────────────────────────────────

    #[test]
    fn stress_1000_cycles_no_leaks() {
        let mut ht = HandleTable::new();
        for cycle in 0..1000 {
            let id = ht.create_handle(KernelObject::Session(format!("srv_{cycle}"))).unwrap();
            ht.get_handle(id, &KernelObject::Session("".into())).unwrap();
            ht.release_handle(id).unwrap();
            ht.release_handle(id).unwrap(); // release both refs
        }
        assert_eq!(ht.active_handles(), 0);
        ht.verify_no_leaks();
    }

    #[test]
    fn stress_10000_create_get_release_no_leaks() {
        let mut ht = HandleTable::new();
        // We create, get, and release 10000 handles — not simultaneously,
        // but sequentially through the free-list recycle path.
        for cycle in 0..10000 {
            let id = ht.create_handle(KernelObject::SharedMemory { size: 4096 }).unwrap();
            ht.get_handle(id, &KernelObject::SharedMemory { size: 0 }).unwrap();
            ht.release_handle(id).unwrap();
            ht.release_handle(id).unwrap();
            if cycle % 1000 == 0 {
                assert_eq!(ht.active_handles(), 0, "leak at cycle {cycle}");
            }
        }
        assert_eq!(ht.active_handles(), 0);
        ht.verify_no_leaks();
    }

    // ── Inspection & type name ─────────────────────────────────────

    #[test]
    fn len_and_capacity() {
        let ht = HandleTable::new();
        assert_eq!(ht.len(), 0);
        assert_eq!(ht.capacity(), 1024);
    }

    #[test]
    fn type_name_returns_correct_strings() {
        assert_eq!(KernelObject::Event.type_name(), "Event");
        assert_eq!(
            KernelObject::Session("ns".into()).type_name(),
            "Session"
        );
        assert_eq!(KernelObject::SharedMemory { size: 0 }.type_name(), "SharedMemory");
        assert_eq!(KernelObject::Process.type_name(), "Process");
        assert_eq!(KernelObject::Thread.type_name(), "Thread");
        assert_eq!(KernelObject::Port.type_name(), "Port");
    }

    #[test]
    fn same_type_as_works() {
        let ev = KernelObject::Event;
        let th = KernelObject::Thread;
        assert!(ev.same_type_as(&KernelObject::Event));
        assert!(!ev.same_type_as(&th));

        let s1 = KernelObject::Session("a".into());
        let s2 = KernelObject::Session("b".into());
        assert!(s1.same_type_as(&s2));
    }

    #[test]
    fn dump_handles_lists_active_entries() {
        let mut ht = HandleTable::new();
        let _id1 = ht.create_handle(KernelObject::Event).unwrap();
        let _id2 = ht.create_handle(KernelObject::Thread).unwrap();
        let dump = ht.dump_handles();
        assert_eq!(dump.len(), 2);
    }

    // ── free_slots ─────────────────────────────────────────────────

    #[test]
    fn free_slots_counts_correctly() {
        let mut ht = HandleTable::new();
        assert_eq!(ht.free_slots(), 1024);
        let id = ht.create_handle(KernelObject::Event).unwrap();
        assert_eq!(ht.free_slots(), 1023);
        ht.release_handle(id).unwrap();
        assert_eq!(ht.free_slots(), 1024);
    }

    // ── Interleaved Create/Release ─────────────────────────────────

    #[test]
    fn interleaved_create_and_release() {
        let mut ht = HandleTable::new();
        let ids: Vec<u32> = (0..50)
            .map(|_| ht.create_handle(KernelObject::Port).unwrap())
            .collect();
        assert_eq!(ht.active_handles(), 50);

        // Release every other one
        for (i, &id) in ids.iter().enumerate() {
            if i % 2 == 0 {
                ht.release_handle(id).unwrap();
            }
        }
        assert_eq!(ht.active_handles(), 25);

        // Create new ones — should reuse freed slots
        for _ in 0..25 {
            ht.create_handle(KernelObject::Port).unwrap();
        }
        assert_eq!(ht.active_handles(), 50);

        // Release all
        for _ in 0..50 {
            // We know there are 50 active handles; release by iterating the dump
            let dump: Vec<u32> = ht.dump_handles().into_iter().map(|(id, _, _)| id).collect();
            for id in dump {
                ht.release_handle(id).unwrap();
            }
        }
        assert_eq!(ht.active_handles(), 0);
        ht.verify_no_leaks();
    }
}
