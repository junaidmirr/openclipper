#[cfg(target_os = "windows")]
pub mod hook {
    use crate::EngineEvent;
    use crate::clipboard::ClipboardService;
    use crate::input::simulate;
    use crate::settings::SharedSettings;
    use crate::slots::SharedSlotManager;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::thread;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN,
        WM_SYSKEYDOWN,
    };

    const VK_ESCAPE: u32 = 0x1B;
    const VK_SPACE: u32 = 0x20;
    const VK_0: u32 = 0x30;
    const VK_1: u32 = 0x31;
    const VK_2: u32 = 0x32;
    const VK_3: u32 = 0x33;
    const VK_4: u32 = 0x34;
    const VK_5: u32 = 0x35;
    const VK_6: u32 = 0x36;
    const VK_7: u32 = 0x37;
    const VK_8: u32 = 0x38;
    const VK_9: u32 = 0x39;
    const VK_C: u32 = 0x43;
    const VK_V: u32 = 0x56;
    const VK_X: u32 = 0x58;
    const VK_SHIFT: u32 = 0x10;
    const VK_CONTROL: u32 = 0x11;

    static mut HOOK_CTX: Option<Arc<WindowsHookContext>> = None;

    pub struct WindowsHookContext {
        pub event_sink: Arc<dyn Fn(EngineEvent) + Send + Sync>,
        pub slot_manager: SharedSlotManager,
        pub clipboard_service: Arc<ClipboardService>,
        pub settings: SharedSettings,
        pub is_copy_mode: Arc<AtomicBool>,
        pub is_cut_mode: Arc<AtomicBool>,
        pub is_paste_mode: Arc<AtomicBool>,
        pub is_explicit_selection: Arc<AtomicBool>,
        pub last_c_tap_ms: Arc<AtomicU64>,
        pub last_x_tap_ms: Arc<AtomicU64>,
        pub last_v_tap_ms: Arc<AtomicU64>,
    }

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
        let ctx = Arc::new(WindowsHookContext {
            event_sink: event_sink.clone(),
            slot_manager,
            clipboard_service,
            settings,
            is_copy_mode,
            is_cut_mode,
            is_paste_mode,
            is_explicit_selection,
            last_c_tap_ms: Arc::new(AtomicU64::new(0)),
            last_x_tap_ms: Arc::new(AtomicU64::new(0)),
            last_v_tap_ms: Arc::new(AtomicU64::new(0)),
        });

        unsafe {
            HOOK_CTX = Some(ctx);
        }

        thread::spawn(move || unsafe {
            let hook: HHOOK = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(low_level_keyboard_proc),
                std::ptr::null_mut(),
                0,
            );

            if hook.is_null() {
                eprintln!("[OpenClipper Engine] Failed to set Windows keyboard hook");
                (event_sink)(EngineEvent::AccessibilityStatus(false));
                return;
            }

            (event_sink)(EngineEvent::AccessibilityStatus(true));
            println!("[OpenClipper Engine] Windows Low-Level Keyboard Hook active!");

            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            UnhookWindowsHookEx(hook);
        });
    }

    unsafe extern "system" fn low_level_keyboard_proc(
        n_code: i32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        unsafe {
            if n_code < 0 {
                return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param);
            }

            let ctx = match &HOOK_CTX {
                Some(c) => c,
                None => return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param),
            };

            let is_key_down = w_param as u32 == WM_KEYDOWN || w_param as u32 == WM_SYSKEYDOWN;
            if !is_key_down {
                return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param);
            }

            let kbd = *(l_param as *const KBDLLHOOKSTRUCT);
            let vk = kbd.vkCode;

            let ctrl_down = (GetAsyncKeyState(VK_CONTROL as i32) as u16 & 0x8000) != 0;
            let shift_down = (GetAsyncKeyState(VK_SHIFT as i32) as u16 & 0x8000) != 0;

            let (double_tap, timeout, slot_count, auto_cut_fallback, auto_paste_pop) = {
                let s = ctx.settings.read().unwrap();
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
            if ctrl_down && shift_down && vk == VK_SPACE {
                (ctx.event_sink)(EngineEvent::ToggleHUD);
                return 1;
            }

            // 1. Copy Mode: Ctrl+Shift+C
            if ctrl_down && shift_down && vk == VK_C {
                let last = ctx.last_c_tap_ms.load(Ordering::SeqCst);
                let is_dt = double_tap && now_ms.saturating_sub(last) < timeout;
                ctx.last_c_tap_ms.store(now_ms, Ordering::SeqCst);

                let cur = ctx.is_copy_mode.load(Ordering::SeqCst);
                if cur {
                    ctx.is_copy_mode.store(false, Ordering::SeqCst);
                    (ctx.event_sink)(EngineEvent::ModeChanged {
                        mode: "idle".into(),
                        info: "AUTO MODE".into(),
                    });
                } else {
                    ctx.is_copy_mode.store(true, Ordering::SeqCst);
                    ctx.is_cut_mode.store(false, Ordering::SeqCst);
                    ctx.is_paste_mode.store(false, Ordering::SeqCst);
                    (ctx.event_sink)(EngineEvent::ShowHUD);
                    (ctx.event_sink)(EngineEvent::ModeChanged {
                        mode: "copy".into(),
                        info: format!("COPY MODE: Select text & tap 1-{}", slot_count),
                    });
                }

                if is_dt {
                    return 1;
                }
                return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param);
            }

            // 2. Cut Mode: Ctrl+Shift+X
            if ctrl_down && shift_down && vk == VK_X {
                let last = ctx.last_x_tap_ms.load(Ordering::SeqCst);
                let is_dt = double_tap && now_ms.saturating_sub(last) < timeout;
                ctx.last_x_tap_ms.store(now_ms, Ordering::SeqCst);

                let cur = ctx.is_cut_mode.load(Ordering::SeqCst);
                if cur {
                    ctx.is_cut_mode.store(false, Ordering::SeqCst);
                    (ctx.event_sink)(EngineEvent::ModeChanged {
                        mode: "idle".into(),
                        info: "AUTO MODE".into(),
                    });
                } else {
                    ctx.is_cut_mode.store(true, Ordering::SeqCst);
                    ctx.is_copy_mode.store(false, Ordering::SeqCst);
                    ctx.is_paste_mode.store(false, Ordering::SeqCst);
                    (ctx.event_sink)(EngineEvent::ShowHUD);
                    (ctx.event_sink)(EngineEvent::ModeChanged {
                        mode: "cut".into(),
                        info: format!("CUT MODE: Select item & tap 1-{}", slot_count),
                    });
                }

                if is_dt {
                    return 1;
                }
                return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param);
            }

            // 3. Paste Mode: Ctrl+Shift+V
            if ctrl_down && shift_down && vk == VK_V {
                let last = ctx.last_v_tap_ms.load(Ordering::SeqCst);
                let is_dt = double_tap && now_ms.saturating_sub(last) < timeout;
                ctx.last_v_tap_ms.store(now_ms, Ordering::SeqCst);

                let cur = ctx.is_paste_mode.load(Ordering::SeqCst);
                if cur {
                    ctx.is_paste_mode.store(false, Ordering::SeqCst);
                    (ctx.event_sink)(EngineEvent::ModeChanged {
                        mode: "idle".into(),
                        info: "AUTO MODE".into(),
                    });
                } else {
                    ctx.is_paste_mode.store(true, Ordering::SeqCst);
                    ctx.is_copy_mode.store(false, Ordering::SeqCst);
                    ctx.is_cut_mode.store(false, Ordering::SeqCst);
                    (ctx.event_sink)(EngineEvent::ShowHUD);
                    (ctx.event_sink)(EngineEvent::ModeChanged {
                        mode: "paste".into(),
                        info: format!("PASTE MODE: Tap 1-{} to paste", slot_count),
                    });
                }

                if is_dt {
                    return 1;
                }
                return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param);
            }

            // Active Modes Key Swallowing (1..N)
            if ctx.is_copy_mode.load(Ordering::SeqCst) {
                if vk == VK_ESCAPE {
                    ctx.is_copy_mode.store(false, Ordering::SeqCst);
                    (ctx.event_sink)(EngineEvent::ModeChanged {
                        mode: "idle".into(),
                        info: "AUTO MODE".into(),
                    });
                    return 1;
                }
                if let Some(digit) = vk_to_digit(vk) {
                    if digit <= slot_count {
                        let slot_idx = digit - 1;
                        let sink = ctx.event_sink.clone();
                        let sm = ctx.slot_manager.clone();
                        let cs = ctx.clipboard_service.clone();

                        thread::spawn(move || {
                            simulate::simulate_copy();
                            thread::sleep(Duration::from_millis(70));
                            if let Some(current) = cs.read_current() {
                                {
                                    let mut m = sm.lock().unwrap();
                                    m.assign(slot_idx, current);
                                }
                                (sink)(EngineEvent::SlotsUpdated);
                                (sink)(EngineEvent::ModeChanged {
                                    mode: "copy".into(),
                                    info: format!("Saved to Slot {}!", digit),
                                });
                            }
                        });

                        return 1;
                    }
                }
            }

            if ctx.is_cut_mode.load(Ordering::SeqCst) {
                if vk == VK_ESCAPE {
                    ctx.is_cut_mode.store(false, Ordering::SeqCst);
                    (ctx.event_sink)(EngineEvent::ModeChanged {
                        mode: "idle".into(),
                        info: "AUTO MODE".into(),
                    });
                    return 1;
                }
                if let Some(digit) = vk_to_digit(vk) {
                    if digit <= slot_count {
                        let slot_idx = digit - 1;
                        let sink = ctx.event_sink.clone();
                        let sm = ctx.slot_manager.clone();
                        let cs = ctx.clipboard_service.clone();

                        thread::spawn(move || {
                            let before = cs.read_current();
                            simulate::simulate_cut();
                            thread::sleep(Duration::from_millis(75));
                            let after_cut = cs.read_current();
                            let (content, was_cut) = if after_cut.is_some() && after_cut != before {
                                (after_cut, true)
                            } else if auto_cut_fallback {
                                simulate::simulate_copy();
                                thread::sleep(Duration::from_millis(75));
                                (cs.read_current(), false)
                            } else {
                                (None, false)
                            };

                            if let Some(item) = content {
                                {
                                    let mut m = sm.lock().unwrap();
                                    m.assign(slot_idx, item);
                                }
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

                        return 1;
                    }
                }
            }

            if ctx.is_paste_mode.load(Ordering::SeqCst) {
                if vk == VK_ESCAPE {
                    ctx.is_paste_mode.store(false, Ordering::SeqCst);
                    (ctx.event_sink)(EngineEvent::ModeChanged {
                        mode: "idle".into(),
                        info: "AUTO MODE".into(),
                    });
                    return 1;
                }
                if let Some(digit) = vk_to_digit(vk) {
                    if digit <= slot_count {
                        let slot_idx = digit - 1;
                        let sm = ctx.slot_manager.clone();
                        let cs = ctx.clipboard_service.clone();

                        thread::spawn(move || {
                            let item = sm.lock().unwrap().get(slot_idx);
                            if let Some(it) = item {
                                let _ = cs.write_content(&it.content);
                                simulate::simulate_paste();
                            }
                        });

                        return 1;
                    }
                }
            }

            // Auto Mode Conveyor Belt Paste on Ctrl+V
            if !ctx.is_copy_mode.load(Ordering::SeqCst)
                && !ctx.is_cut_mode.load(Ordering::SeqCst)
                && !ctx.is_paste_mode.load(Ordering::SeqCst)
            {
                if ctrl_down && !shift_down && vk == VK_V {
                    let was_explicit = ctx.is_explicit_selection.swap(false, Ordering::SeqCst);
                    if !was_explicit {
                        let has_items = ctx.slot_manager.lock().unwrap().has_auto_items();
                        if has_items {
                            let popped = ctx
                                .slot_manager
                                .lock()
                                .unwrap()
                                .auto_paste_pop_custom(auto_paste_pop);
                            if let Some(item) = popped {
                                let _ = ctx.clipboard_service.write_content(&item);
                                (ctx.event_sink)(EngineEvent::SlotsUpdated);
                            }
                        }
                    }
                }
            }

            CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param)
        }
    }

    fn vk_to_digit(vk: u32) -> Option<usize> {
        match vk {
            VK_1 => Some(1),
            VK_2 => Some(2),
            VK_3 => Some(3),
            VK_4 => Some(4),
            VK_5 => Some(5),
            VK_6 => Some(6),
            VK_7 => Some(7),
            VK_8 => Some(8),
            VK_9 => Some(9),
            VK_0 => Some(10),
            _ => None,
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub mod hook {
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
