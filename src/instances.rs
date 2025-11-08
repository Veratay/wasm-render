#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use crate::batcher::MATRIX_FLOATS;

#[derive(Debug)]
pub(crate) struct InstanceStore {
    entries: Vec<Option<InstanceRecord>>,
    free_list: Vec<u32>,
    active_handles: Vec<u32>,
}

#[derive(Debug)]
pub(crate) struct InstanceRecord {
    pub(crate) mesh_index: usize,
    pub(crate) slot_index: usize,
    pub(crate) transform: [f32; MATRIX_FLOATS],
    active_slot: usize,
}

impl InstanceStore {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            free_list: Vec::new(),
            active_handles: Vec::new(),
        }
    }

    pub(crate) fn insert(
        &mut self,
        mesh_index: usize,
        slot_index: usize,
        transform: [f32; MATRIX_FLOATS],
    ) -> u32 {
        let handle = self.free_list.pop().unwrap_or_else(|| {
            let next = self.entries.len() as u32;
            self.entries.push(None);
            next
        });

        let slot = self.active_handles.len();
        self.active_handles.push(handle);
        self.entries[handle as usize] = Some(InstanceRecord {
            mesh_index,
            slot_index,
            transform,
            active_slot: slot,
        });
        handle
    }

    pub(crate) fn get(&self, handle: u32) -> Option<&InstanceRecord> {
        self.entries.get(handle as usize)?.as_ref()
    }

    pub(crate) fn get_mut(&mut self, handle: u32) -> Option<&mut InstanceRecord> {
        self.entries.get_mut(handle as usize)?.as_mut()
    }

    pub(crate) fn remove(&mut self, handle: u32) -> bool {
        let entry = match self.entries.get_mut(handle as usize) {
            Some(entry) => entry,
            None => return false,
        };
        let record = match entry.take() {
            Some(record) => record,
            None => return false,
        };

        let slot = record.active_slot;
        if let Some(last_handle) = self.active_handles.pop() {
            if last_handle != handle {
                if let Some(target_slot) = self.active_handles.get_mut(slot) {
                    *target_slot = last_handle;
                }
                if let Some(last_record) = self.entries[last_handle as usize].as_mut() {
                    last_record.active_slot = slot;
                }
            }
        }

        self.free_list.push(handle);
        true
    }

    pub(crate) fn len(&self) -> usize {
        self.active_handles.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.active_handles.is_empty()
    }

    pub(crate) fn handle_at(&self, index: usize) -> Option<u32> {
        self.active_handles.get(index).copied()
    }
}
