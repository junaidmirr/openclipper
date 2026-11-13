#[cfg(target_os = "linux")]
pub mod linux_hook {
    use crate::EngineEvent;
    use crate::clipboard::ClipboardService;
    use crate::input::simulate;
    use crate::settings::SharedSettings;
    use crate::slots::SharedSlotManager;
    use rdev::{Event, EventType, Key};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::thread;
    use std::time::Duration;

    pub fn start_hook(
        event_sink: Arc<dyn Fn(EngineEvent) + Send + Sync>,
        slot_manager: SharedSlotManager,
        clipboard_service: Arc<ClipboardService>,
        settings: SharedSettings,
        is_copy_mode: Arc<AtomicBool>,
        is_cut_mode: Arc<AtomicBool>,
        is_paste_mode: Arc<AtomicBool>,
        is_explicit_selection: Arc<AtomicBool>,
    ) {
        let last_c_tap_ms = Arc::new(AtomicU64::new(0));
        let last_x_tap_ms = Arc::new(AtomicU64::new(0));
        let last_v_tap_ms = Arc::new(AtomicU64::new(0));
        let ctrl_pressed = Arc::new(AtomicBool::new(false));
        let shift_pressed = Arc::new(AtomicBool::new(false));

        (event_sink)(EngineEvent::AccessibilityStatus(true));

        thread::spawn(move || {
            let callback = move |event: Event| {
                let is_down = matches!(event.event_type, EventType::KeyPress(_));
                let is_up = matches!(event.event_type, EventType::KeyRelease(_));

                match event.event_type {
                    EventType::KeyPress(Key::ControlLeft)
                    | EventType::KeyPress(Key::ControlRight) => {
                        ctrl_pressed.store(true, Ordering::SeqCst);
                    }
                    EventType::KeyRelease(Key::ControlLeft)
                    | EventType::KeyRelease(Key::ControlRight) => {
                        ctrl_pressed.store(false, Ordering::SeqCst);
                    }
                    EventType::KeyPress(Key::ShiftLeft) | EventType::KeyPress(Key::ShiftRight) => {
                        shift_pressed.store(true, Ordering::SeqCst);
                    }
                    EventType::KeyRelease(Key::ShiftLeft)
                    | EventType::KeyRelease(Key::ShiftRight) => {
                        shift_pressed.store(false, Ordering::SeqCst);
                    }
                    _ => {}
                }

                if !is_down {
                    return;
                }

                let key = match event.event_type {
                    EventType::KeyPress(k) => k,
                    _ => return,
                };

                let ctrl = ctrl_pressed.load(Ordering::SeqCst);
                let shift = shift_pressed.load(Ordering::SeqCst);
                let (double_tap, timeout, slot_count, auto_cut_fallback, auto_paste_pop) = {
                    let s = settings.read().unwrap();
                    (
                        s.double_tap_enabled,
                        s.double_tap_timeout_ms,
                        s.slot_count,
                        s.auto_cut_fallback,
                        s.auto_paste_pop,
                    )
                };

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                // Toggle HUD on Ctrl+Shift+Space
                if ctrl && shift && key == Key::Space {
                    (event_sink)(EngineEvent::ToggleHUD);
                    return;
                }

                // 1. Copy Mode on Ctrl+Shift+C
                if ctrl && shift && key == Key::KeyC {
                    let last = last_c_tap_ms.load(Ordering::SeqCst);
                    last_c_tap_ms.store(now_ms, Ordering::SeqCst);

                    let cur = is_copy_mode.load(Ordering::SeqCst);
                    if cur {
                        is_copy_mode.store(false, Ordering::SeqCst);
                        (event_sink)(EngineEvent::ModeChanged {
                            mode: "idle".into(),
                            info: "AUTO MODE".into(),
                        });
                    } else {
                        is_copy_mode.store(true, Ordering::SeqCst);
                        is_cut_mode.store(false, Ordering::SeqCst);
                        is_paste_mode.store(false, Ordering::SeqCst);
                        (event_sink)(EngineEvent::ShowHUD);
                        (event_sink)(EngineEvent::ModeChanged {
                            mode: "copy".into(),
                            info: format!("COPY MODE: Select text & tap 1-{}", slot_count),
                        });
                    }
                    return;
                }

                // 2. Cut Mode on Ctrl+Shift+X
                if ctrl && shift && key == Key::KeyX {
                    let last = last_x_tap_ms.load(Ordering::SeqCst);
                    last_x_tap_ms.store(now_ms, Ordering::SeqCst);

                    let cur = is_cut_mode.load(Ordering::SeqCst);
                    if cur {
                        is_cut_mode.store(false, Ordering::SeqCst);
                        (event_sink)(EngineEvent::ModeChanged {
                            mode: "idle".into(),
                            info: "AUTO MODE".into(),
                        });
                    } else {
                        is_cut_mode.store(true, Ordering::SeqCst);
                        is_copy_mode.store(false, Ordering::SeqCst);
                        is_paste_mode.store(false, Ordering::SeqCst);
                        (event_sink)(EngineEvent::ShowHUD);
                        (event_sink)(EngineEvent::ModeChanged {
                            mode: "cut".into(),
                            info: format!("CUT MODE: Select item & tap 1-{}", slot_count),
                        });
                    }
                    return;
                }

                // 3. Paste Mode on Ctrl+Shift+V
                if ctrl && shift && key == Key::KeyV {
                    let last = last_v_tap_ms.load(Ordering::SeqCst);
                    last_v_tap_ms.store(now_ms, Ordering::SeqCst);

                    let cur = is_paste_mode.load(Ordering::SeqCst);
                    if cur {
                        is_paste_mode.store(false, Ordering::SeqCst);
                        (event_sink)(EngineEvent::ModeChanged {
                            mode: "idle".into(),
                            info: "AUTO MODE".into(),
                        });
                    } else {
                        is_paste_mode.store(true, Ordering::SeqCst);
                        is_copy_mode.store(false, Ordering::SeqCst);
                        is_cut_mode.store(false, Ordering::SeqCst);
                        (event_sink)(EngineEvent::ShowHUD);
                        (event_sink)(EngineEvent::ModeChanged {
                            mode: "paste".into(),
                            info: format!("PASTE MODE: Tap 1-{} to paste", slot_count),
                        });
                    }
                    return;
                }

                // Esc to cancel mode
                if key == Key::Escape {
                    if is_copy_mode.load(Ordering::SeqCst)
                        || is_cut_mode.load(Ordering::SeqCst)
                        || is_paste_mode.load(Ordering::SeqCst)
                    {
                        is_copy_mode.store(false, Ordering::SeqCst);
                        is_cut_mode.store(false, Ordering::SeqCst);
                        is_paste_mode.store(false, Ordering::SeqCst);
                        (event_sink)(EngineEvent::ModeChanged {
                            mode: "idle".into(),
                            info: "AUTO MODE".into(),
                        });
                        return;
                    }
                }

                // Slot digit selection 1..slot_count
                if let Some(digit) = key_to_digit(key) {
                    if digit <= slot_count {
                        let slot_idx = digit - 1;

                        if is_copy_mode.load(Ordering::SeqCst) {
                            let sink = event_sink.clone();
                            let sm = slot_manager.clone();
                            let cs = clipboard_service.clone();
                            thread::spawn(move || {
                                simulate::simulate_copy();
                                thread::sleep(Duration::from_millis(70));
                                if let Some(current) = cs.read_current() {
                                    sm.lock().unwrap().assign(slot_idx, current);
                                    (sink)(EngineEvent::SlotsUpdated);
                                    (sink)(EngineEvent::ModeChanged {
                                        mode: "copy".into(),
                                        info: format!("Saved to Slot {}!", digit),
                                    });
                                }
                            });
                            return;
                        }

                        if is_cut_mode.load(Ordering::SeqCst) {
                            let sink = event_sink.clone();
                            let sm = slot_manager.clone();
                            let cs = clipboard_service.clone();
                            thread::spawn(move || {
                                let before = cs.read_current();
                                simulate::simulate_cut();
                                thread::sleep(Duration::from_millis(75));
                                let after_cut = cs.read_current();
                                let (content, was_cut) =
                                    if after_cut.is_some() && after_cut != before {
                                        (after_cut, true)
                                    } else if auto_cut_fallback {
                                        simulate::simulate_copy();
                                        thread::sleep(Duration::from_millis(75));
                                        (cs.read_current(), false)
                                    } else {
                                        (None, false)
                                    };

                                if let Some(item) = content {
                                    sm.lock().unwrap().assign(slot_idx, item);
                                    (sink)(EngineEvent::SlotsUpdated);
                                    let info = if was_cut {
                                        format!("Cut to Slot {}!", digit)
                                    } else {
                                        format!("Copied to Slot {} (Cut fallback)!", digit)
                                    };
                                    (sink)(EngineEvent::ModeChanged {
                                        mode: "cut".into(),
                                        info,
                                    });
                                }
                            });
                            return;
                        }

                        if is_paste_mode.load(Ordering::SeqCst) {
                            let item = slot_manager.lock().unwrap().get(slot_idx);
                            let cs = clipboard_service.clone();
                            thread::spawn(move || {
                                if let Some(it) = item {
                                    let _ = cs.write_content(&it.content);
                                    simulate::simulate_paste();
                                }
                            });
                            return;
                        }
                    }
                }

                // Auto Mode Conveyor Paste on Ctrl+V
                if !is_copy_mode.load(Ordering::SeqCst)
                    && !is_cut_mode.load(Ordering::SeqCst)
                    && !is_paste_mode.load(Ordering::SeqCst)
                {
                    if ctrl && !shift && key == Key::KeyV {
                        let was_explicit = is_explicit_selection.swap(false, Ordering::SeqCst);
                        if !was_explicit {
                            let has_items = slot_manager.lock().unwrap().has_auto_items();
                            if has_items {
                                let popped = slot_manager
                                    .lock()
                                    .unwrap()
                                    .auto_paste_pop_custom(auto_paste_pop);
                                if let Some(item) = popped {
                                    let _ = clipboard_service.write_content(&item);
                                    (event_sink)(EngineEvent::SlotsUpdated);
                                }
                            }
                        }
                    }
                }
            };

            if let Err(err) = rdev::listen(callback) {
                eprintln!("[OpenClipper Engine] Linux rdev error: {:?}", err);
            }
        });
    }

    fn key_to_digit(key: Key) -> Option<usize> {
        match key {
            Key::Num1 => Some(1),
            Key::Num2 => Some(2),
            Key::Num3 => Some(3),
            Key::Num4 => Some(4),
            Key::Num5 => Some(5),
            Key::Num6 => Some(6),
            Key::Num7 => Some(7),
            Key::Num8 => Some(8),
            Key::Num9 => Some(9),
            Key::Num0 => Some(10),
            _ => None,
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub mod linux_hook {
    use crate::EngineEvent;
    use crate::clipboard::ClipboardService;
    use crate::settings::SharedSettings;
    use crate::slots::SharedSlotManager;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    pub fn start_hook(
        _event_sink: Arc<dyn Fn(EngineEvent) + Send + Sync>,
        _slot_manager: SharedSlotManager,
        _clipboard_service: Arc<ClipboardService>,
        _settings: SharedSettings,
        _is_copy_mode: Arc<AtomicBool>,
        _is_cut_mode: Arc<AtomicBool>,
        _is_paste_mode: Arc<AtomicBool>,
        _is_explicit_selection: Arc<AtomicBool>,
    ) {
    }
}
