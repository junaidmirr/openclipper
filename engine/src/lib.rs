pub mod clipboard;
pub mod input;
pub mod settings;
pub mod slots;

use clipboard::ClipboardService;
use serde::{Deserialize, Serialize};
use settings::{AppSettings, SharedSettings};
use slots::{SharedSlotManager, SlotContent, SlotItem, SlotManager};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EngineEvent {
    SlotsUpdated,
    SlotSelected { slot_index: usize },
    ModeChanged { mode: String, info: String },
    AccessibilityStatus(bool),
    ToggleHUD,
    ShowHUD,
    HideHUD,
}

pub struct Engine {
    pub slot_manager: SharedSlotManager,
    pub clipboard_service: Arc<ClipboardService>,
    pub settings: SharedSettings,
    pub is_copy_mode: Arc<AtomicBool>,
    pub is_cut_mode: Arc<AtomicBool>,
    pub is_paste_mode: Arc<AtomicBool>,
    pub is_explicit_selection: Arc<AtomicBool>,
    pub event_sink: Arc<dyn Fn(EngineEvent) + Send + Sync>,
}

impl Engine {
    pub fn new<F>(event_sink: F) -> Result<Self, String>
    where
        F: Fn(EngineEvent) + Send + Sync + 'static,
    {
        let initial_settings = AppSettings::load();
        let slot_count = initial_settings.slot_count;

        let mut slot_mgr = SlotManager::with_capacity(slot_count);
        if initial_settings.persist_slots {
            slot_mgr.load_from_file(&AppSettings::slots_storage_path());
        }

        let slot_manager = Arc::new(Mutex::new(slot_mgr));
        let clipboard_service = Arc::new(ClipboardService::new()?);
        let settings = Arc::new(RwLock::new(initial_settings));

        let is_paste_mode = Arc::new(AtomicBool::new(false));
        let is_copy_mode = Arc::new(AtomicBool::new(false));
        let is_cut_mode = Arc::new(AtomicBool::new(false));
        let is_explicit_selection = Arc::new(AtomicBool::new(false));

        Ok(Self {
            slot_manager,
            clipboard_service,
            settings,
            is_copy_mode,
            is_cut_mode,
            is_paste_mode,
            is_explicit_selection,
            event_sink: Arc::new(event_sink),
        })
    }

    /// Start the background clipboard watcher and native platform input hooks
    pub fn start(&self) {
        // 1. Background Clipboard Watcher Thread
        let sink_watch = self.event_sink.clone();
        let sm_watch = self.slot_manager.clone();
        let cs_watch = self.clipboard_service.clone();
        let settings_watch = self.settings.clone();
        let is_copy_watch = self.is_copy_mode.clone();
        let is_cut_watch = self.is_cut_mode.clone();

        thread::spawn(move || {
            let mut last_content: Option<SlotContent> = None;
            loop {
                thread::sleep(Duration::from_millis(180));

                if !is_copy_watch.load(Ordering::SeqCst) && !is_cut_watch.load(Ordering::SeqCst) {
                    if let Some(current) = cs_watch.read_external() {
                        let is_new = match &last_content {
                            Some(last) => last != &current,
                            None => true,
                        };

                        if is_new {
                            last_content = Some(current.clone());
                            let (direction, show_on_copy, persist) = {
                                let s = settings_watch.read().unwrap();
                                (
                                    s.auto_copy_direction.clone(),
                                    s.show_hud_on_copy,
                                    s.persist_slots,
                                )
                            };

                            {
                                let mut sm = sm_watch.lock().unwrap();
                                sm.auto_copy_directional(current, &direction);
                                if persist {
                                    let _ = sm.save_to_file(&AppSettings::slots_storage_path());
                                }
                            }
                            (sink_watch)(EngineEvent::SlotsUpdated);

                            if show_on_copy {
                                (sink_watch)(EngineEvent::ShowHUD);
                            }
                        }
                    }
                }
            }
        });

        // 2. Start Native Platform Keyboard Hook
        input::start_input_hook(
            self.event_sink.clone(),
            self.slot_manager.clone(),
            self.clipboard_service.clone(),
            self.settings.clone(),
            self.is_copy_mode.clone(),
            self.is_cut_mode.clone(),
            self.is_paste_mode.clone(),
            self.is_explicit_selection.clone(),
        );
    }

    pub fn get_slots(&self) -> (Vec<Option<SlotItem>>, usize, usize) {
        let sm = self.slot_manager.lock().unwrap();
        (sm.get_slots(), sm.auto_count(), sm.capacity())
    }

    pub fn select_slot(&self, slot_index: usize) -> Result<(), String> {
        let item = {
            let sm = self.slot_manager.lock().unwrap();
            sm.get(slot_index)
        };

        if let Some(item) = item {
            self.clipboard_service.write_content(&item.content)?;
            self.is_explicit_selection.store(true, Ordering::SeqCst);

            if self.is_paste_mode.load(Ordering::SeqCst) {
                input::simulate::simulate_paste();
            }

            (self.event_sink)(EngineEvent::SlotSelected { slot_index });
        }

        Ok(())
    }

    pub fn assign_to_slot(&self, slot_index: usize) -> Result<(), String> {
        if let Some(current) = self.clipboard_service.read_current() {
            {
                let mut sm = self.slot_manager.lock().unwrap();
                sm.assign(slot_index, current);
                if self.settings.read().unwrap().persist_slots {
                    let _ = sm.save_to_file(&AppSettings::slots_storage_path());
                }
            }
            (self.event_sink)(EngineEvent::SlotsUpdated);

            self.is_copy_mode.store(false, Ordering::SeqCst);
            self.is_cut_mode.store(false, Ordering::SeqCst);
            (self.event_sink)(EngineEvent::ModeChanged {
                mode: "idle".into(),
                info: format!("Assigned to Slot {}", slot_index + 1),
            });
        }
        Ok(())
    }

    pub fn clear_all_slots(&self) {
        {
            let mut sm = self.slot_manager.lock().unwrap();
            sm.clear();
            if self.settings.read().unwrap().persist_slots {
                let _ = sm.save_to_file(&AppSettings::slots_storage_path());
            }
        }
        self.is_explicit_selection.store(false, Ordering::SeqCst);
        (self.event_sink)(EngineEvent::SlotsUpdated);
    }

    pub fn set_mode(&self, new_mode: &str) {
        let slot_count = self.settings.read().unwrap().slot_count;
        match new_mode {
            "copy" => {
                self.is_copy_mode.store(true, Ordering::SeqCst);
                self.is_cut_mode.store(false, Ordering::SeqCst);
                self.is_paste_mode.store(false, Ordering::SeqCst);
                (self.event_sink)(EngineEvent::ModeChanged {
                    mode: "copy".into(),
                    info: format!("COPY MODE: Select text & tap 1-{}", slot_count),
                });
            }
            "cut" => {
                self.is_cut_mode.store(true, Ordering::SeqCst);
                self.is_copy_mode.store(false, Ordering::SeqCst);
                self.is_paste_mode.store(false, Ordering::SeqCst);
                (self.event_sink)(EngineEvent::ModeChanged {
                    mode: "cut".into(),
                    info: format!("CUT MODE: Select item & tap 1-{}", slot_count),
                });
            }
            "paste" => {
                self.is_paste_mode.store(true, Ordering::SeqCst);
                self.is_copy_mode.store(false, Ordering::SeqCst);
                self.is_cut_mode.store(false, Ordering::SeqCst);
                (self.event_sink)(EngineEvent::ModeChanged {
                    mode: "paste".into(),
                    info: format!("PASTE MODE: Tap 1-{} to paste", slot_count),
                });
            }
            _ => {
                self.is_copy_mode.store(false, Ordering::SeqCst);
                self.is_cut_mode.store(false, Ordering::SeqCst);
                self.is_paste_mode.store(false, Ordering::SeqCst);
                (self.event_sink)(EngineEvent::ModeChanged {
                    mode: "idle".into(),
                    info: "AUTO MODE".into(),
                });
            }
        }
    }

    pub fn get_settings(&self) -> AppSettings {
        self.settings.read().unwrap().clone()
    }

    pub fn update_settings(&self, new_settings: AppSettings) -> Result<(), String> {
        let old_slot_count = self.settings.read().unwrap().slot_count;

        new_settings.save()?;

        if new_settings.slot_count != old_slot_count {
            let mut sm = self.slot_manager.lock().unwrap();
            sm.set_capacity(new_settings.slot_count);
        }

        *self.settings.write().unwrap() = new_settings;
        (self.event_sink)(EngineEvent::SlotsUpdated);

        Ok(())
    }

    pub fn reset_settings(&self) -> Result<AppSettings, String> {
        let defaults = AppSettings::default();
        defaults.save()?;

        {
            let mut sm = self.slot_manager.lock().unwrap();
            sm.set_capacity(defaults.slot_count);
        }

        *self.settings.write().unwrap() = defaults.clone();
        (self.event_sink)(EngineEvent::SlotsUpdated);

        Ok(defaults)
    }

    pub fn check_accessibility(&self) -> bool {
        input::is_accessibility_trusted()
    }

    pub fn prompt_accessibility(&self) -> bool {
        input::prompt_accessibility()
    }
}
