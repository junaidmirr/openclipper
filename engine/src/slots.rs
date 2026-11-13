use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SlotContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data_url: String,
        width: u32,
        height: u32,
        #[serde(default, skip_serializing)]
        full_bytes: Option<Vec<u8>>,
    },
    #[serde(rename = "files")]
    Files { files: Vec<String> },
}

impl PartialEq for SlotContent {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (SlotContent::Text { text: a }, SlotContent::Text { text: b }) => a == b,
            (
                SlotContent::Image {
                    data_url: a,
                    width: w1,
                    height: h1,
                    ..
                },
                SlotContent::Image {
                    data_url: b,
                    width: w2,
                    height: h2,
                    ..
                },
            ) => a == b && w1 == w2 && h1 == h2,
            (SlotContent::Files { files: a }, SlotContent::Files { files: b }) => a == b,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotItem {
    pub slot: usize, // 1..N
    #[serde(flatten)]
    pub content: SlotContent,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct SlotManager {
    capacity: usize,
    slots: Vec<Option<SlotItem>>,
    auto_count: usize,
}

impl SlotManager {
    pub fn new() -> Self {
        Self::with_capacity(9)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let cap = capacity.clamp(3, 15);
        Self {
            capacity: cap,
            slots: vec![None; cap],
            auto_count: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn set_capacity(&mut self, new_capacity: usize) {
        let new_cap = new_capacity.clamp(3, 15);
        if new_cap == self.capacity {
            return;
        }

        let mut new_slots = vec![None; new_cap];
        let copy_limit = self.slots.len().min(new_cap);
        for i in 0..copy_limit {
            if let Some(mut item) = self.slots[i].take() {
                item.slot = i + 1;
                new_slots[i] = Some(item);
            }
        }

        self.capacity = new_cap;
        self.slots = new_slots;
        self.auto_count = self.slots.iter().filter(|s| s.is_some()).count();
    }

    /// Automatic Mode: append newly copied item (FIFO default)
    pub fn auto_copy(&mut self, content: SlotContent) {
        self.auto_copy_directional(content, "fifo");
    }

    /// Automatic Mode with directional policy ("fifo" append vs "lifo" prepend)
    pub fn auto_copy_directional(&mut self, content: SlotContent, direction: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if direction == "lifo" {
            // LIFO / Prepend to Slot 1 and shift others right
            if let Some(Some(first)) = self.slots.first() {
                if first.content == content {
                    return; // Ignore duplicate
                }
            }

            for i in (0..self.capacity - 1).rev() {
                if let Some(mut item) = self.slots[i].take() {
                    item.slot = i + 2;
                    self.slots[i + 1] = Some(item);
                }
            }

            self.slots[0] = Some(SlotItem {
                slot: 1,
                content,
                timestamp: now,
            });

            self.auto_count = (self.auto_count + 1).min(self.capacity);
        } else {
            // FIFO / Append to end of active queue
            if self.auto_count > 0 && self.auto_count <= self.capacity {
                if let Some(recent) = &self.slots[self.auto_count - 1] {
                    if recent.content == content {
                        return; // Ignore duplicate
                    }
                }
            }

            if self.auto_count < self.capacity {
                let slot_num = self.auto_count + 1;
                self.slots[self.auto_count] = Some(SlotItem {
                    slot: slot_num,
                    content,
                    timestamp: now,
                });
                self.auto_count += 1;
            } else {
                // Queue full: shift items left, put new item in last slot
                for i in 0..(self.capacity - 1) {
                    if let Some(mut item) = self.slots[i + 1].take() {
                        item.slot = i + 1;
                        self.slots[i] = Some(item);
                    }
                }
                self.slots[self.capacity - 1] = Some(SlotItem {
                    slot: self.capacity,
                    content,
                    timestamp: now,
                });
            }
        }
    }

    /// Pop the first item (Slot 1), shift remaining items forward (Slot 2 -> Slot 1)
    pub fn auto_paste_pop(&mut self) -> Option<SlotContent> {
        self.auto_paste_pop_custom(true)
    }

    /// Custom auto paste: pop & shift if pop_enabled is true, or keep slot if false
    pub fn auto_paste_pop_custom(&mut self, pop_enabled: bool) -> Option<SlotContent> {
        if self.auto_count == 0 {
            return None;
        }

        if !pop_enabled {
            return self.slots[0].as_ref().map(|item| item.content.clone());
        }

        let popped = self.slots[0].take().map(|item| item.content)?;

        for i in 0..(self.capacity - 1) {
            if let Some(mut item) = self.slots[i + 1].take() {
                item.slot = i + 1;
                self.slots[i] = Some(item);
            } else {
                self.slots[i] = None;
            }
        }
        self.slots[self.capacity - 1] = None;
        self.auto_count = self.auto_count.saturating_sub(1);

        Some(popped)
    }

    /// Targeted assign to a specific slot (0-indexed)
    pub fn assign(&mut self, slot_index: usize, content: SlotContent) {
        if slot_index >= self.capacity {
            return;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.slots[slot_index] = Some(SlotItem {
            slot: slot_index + 1,
            content,
            timestamp: now,
        });

        let max_filled = self.slots.iter().filter(|s| s.is_some()).count();
        self.auto_count = max_filled;
    }

    /// Get all slots
    pub fn get_slots(&self) -> Vec<Option<SlotItem>> {
        self.slots.clone()
    }

    /// Get specific slot (0-indexed)
    pub fn get(&self, slot_index: usize) -> Option<SlotItem> {
        if slot_index < self.capacity {
            self.slots[slot_index].clone()
        } else {
            None
        }
    }

    /// Check if auto queue has items
    pub fn has_auto_items(&self) -> bool {
        self.auto_count > 0 && self.slots.first().and_then(|s| s.as_ref()).is_some()
    }

    /// Get count of items in auto queue
    pub fn auto_count(&self) -> usize {
        self.auto_count
    }

    /// Clear all slots
    pub fn clear(&mut self) {
        self.slots = vec![None; self.capacity];
        self.auto_count = 0;
    }

    /// Save slots to JSON file for persistence
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string(&self.slots).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Load slots from JSON file
    pub fn load_from_file(&mut self, path: &Path) {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(loaded) = serde_json::from_str::<Vec<Option<SlotItem>>>(&content) {
                    let mut new_slots = vec![None; self.capacity];
                    let limit = loaded.len().min(self.capacity);
                    for i in 0..limit {
                        if let Some(mut item) = loaded[i].clone() {
                            item.slot = i + 1;
                            new_slots[i] = Some(item);
                        }
                    }
                    self.slots = new_slots;
                    self.auto_count = self.slots.iter().filter(|s| s.is_some()).count();
                }
            }
        }
    }
}

pub type SharedSlotManager = Arc<Mutex<SlotManager>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_copy_and_pop() {
        let mut sm = SlotManager::new();

        sm.auto_copy(SlotContent::Text {
            text: "Item 1".into(),
        });
        sm.auto_copy(SlotContent::Text {
            text: "Item 2".into(),
        });

        assert_eq!(sm.auto_count(), 2);

        let pop1 = sm.auto_paste_pop();
        assert_eq!(pop1, Some(SlotContent::Text { text: "Item 1".into() }));
        assert_eq!(sm.auto_count(), 1);

        let pop2 = sm.auto_paste_pop();
        assert_eq!(pop2, Some(SlotContent::Text { text: "Item 2".into() }));
        assert_eq!(sm.auto_count(), 0);

        let pop3 = sm.auto_paste_pop();
        assert_eq!(pop3, None);
    }

    #[test]
    fn test_clear_slots() {
        let mut sm = SlotManager::new();
        sm.auto_copy(SlotContent::Text { text: "ABC".into() });
        assert_eq!(sm.auto_count(), 1);

        sm.clear();
        assert_eq!(sm.auto_count(), 0);
        assert_eq!(sm.get(0), None);
    }

    #[test]
    fn test_targeted_assignment() {
        let mut sm = SlotManager::new();
        sm.assign(
            3,
            SlotContent::Text {
                text: "Slot 4 item".into(),
            },
        );

        let slot4 = sm.get(3);
        assert!(slot4.is_some());
        assert_eq!(
            slot4.unwrap().content,
            SlotContent::Text {
                text: "Slot 4 item".into()
            }
        );
    }

    #[test]
    fn test_dynamic_capacity() {
        let mut sm = SlotManager::with_capacity(5);
        assert_eq!(sm.capacity(), 5);
        assert_eq!(sm.get_slots().len(), 5);

        sm.set_capacity(12);
        assert_eq!(sm.capacity(), 12);
        assert_eq!(sm.get_slots().len(), 12);
    }
}
