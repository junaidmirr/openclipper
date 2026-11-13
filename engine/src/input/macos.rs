#[cfg(target_os = "macos")]
pub mod tap {
    use crate::clipboard::ClipboardService;
    use crate::input::simulate;
    use crate::settings::SharedSettings;
    use crate::slots::SharedSlotManager;
    use crate::EngineEvent;
    use objc2_foundation::{NSDictionary, NSNumber, NSString};
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    unsafe extern "C" {
        pub fn AXIsProcessTrusted() -> bool;
        pub fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            eventsOfInterest: u64,
            callback: CGEventTapCallBack,
            refcon: *mut c_void,
        ) -> *mut c_void;
        fn CFMachPortCreateRunLoopSource(
            allocator: *const c_void,
            port: *mut c_void,
            order: isize,
        ) -> *mut c_void;
        fn CFRunLoopGetCurrent() -> *mut c_void;
        fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: *const c_void);
        fn CFRunLoopRun();
        fn CGEventGetFlags(event: *mut c_void) -> u64;
        fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
        fn CGEventTapEnable(port: *mut c_void, enable: bool);
        static kCFRunLoopCommonModes: *const c_void;
    }

    type CGEventTapCallBack = extern "C" fn(
        proxy: *mut c_void,
        type_: u32,
        event: *mut c_void,
        refcon: *mut c_void,
    ) -> *mut c_void;

    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;
    const K_CG_EVENT_KEY_DOWN: u32 = 10;
    const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 14;
    const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

    // macOS Keycodes
    const VK_ANSI_X: i64 = 7;
    const VK_ANSI_C: i64 = 8;
    const VK_ANSI_V: i64 = 9;
    const VK_ANSI_1: i64 = 18;
    const VK_ANSI_2: i64 = 19;
    const VK_ANSI_3: i64 = 20;
    const VK_ANSI_4: i64 = 21;
    const VK_ANSI_6: i64 = 22;
    const VK_ANSI_5: i64 = 23;
    const VK_ANSI_9: i64 = 25;
    const VK_ANSI_7: i64 = 26;
    const VK_ANSI_8: i64 = 28;
    const VK_ANSI_0: i64 = 29;
    const VK_SPACE: i64 = 49;
    const VK_ESCAPE: i64 = 53;

    // Modifier Masks
    const MASK_CMD: u64 = 0x00100000;
    const MASK_SHIFT: u64 = 0x00020000;
    const MASK_CTRL: u64 = 0x00040000;

    pub struct TapContext {
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
        pub tap_port: Arc<std::sync::atomic::AtomicPtr<c_void>>,
    }

    pub fn is_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    pub fn prompt_accessibility() -> bool {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();

        unsafe {
            let key = NSString::from_str("AXTrustedCheckOptionPrompt");
            let val = NSNumber::numberWithBool(true);
            let dict = NSDictionary::from_slices(&[&*key], &[&*val]);
            let raw_ptr = objc2::rc::Retained::as_ptr(&dict) as *const c_void;
            AXIsProcessTrustedWithOptions(raw_ptr)
        }
    }

    pub fn start_tap(
        event_sink: Arc<dyn Fn(EngineEvent) + Send + Sync>,
        slot_manager: SharedSlotManager,
        clipboard_service: Arc<ClipboardService>,
        settings: SharedSettings,
        is_copy_mode: Arc<AtomicBool>,
        is_cut_mode: Arc<AtomicBool>,
        is_paste_mode: Arc<AtomicBool>,
        is_explicit_selection: Arc<AtomicBool>,
    ) {
        thread::spawn(move || {
            if !is_trusted() {
                prompt_accessibility();
            }

            let ctx = Box::new(TapContext {
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
                tap_port: Arc::new(std::sync::atomic::AtomicPtr::new(std::ptr::null_mut())),
            });

            let ctx_ptr = Box::into_raw(ctx);

            let tap;
            loop {
                let current_tap = unsafe {
                    let mask = 1u64 << K_CG_EVENT_KEY_DOWN;
                    CGEventTapCreate(
                        K_CG_SESSION_EVENT_TAP,
                        K_CG_HEAD_INSERT_EVENT_TAP,
                        K_CG_EVENT_TAP_OPTION_DEFAULT,
                        mask,
                        tap_callback,
                        ctx_ptr as *mut c_void,
                    )
                };

                if !current_tap.is_null() {
                    tap = current_tap;
                    break;
                }

                (event_sink)(EngineEvent::AccessibilityStatus(false));
                thread::sleep(Duration::from_millis(1500));
            }

            (event_sink)(EngineEvent::AccessibilityStatus(true));

            unsafe {
                (*ctx_ptr).tap_port.store(tap, Ordering::SeqCst);

                let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
                if source.is_null() {
                    return;
                }

                let run_loop = CFRunLoopGetCurrent();
                CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);
                println!("[OpenClipper Engine] macOS Active Event Tap attached!");
                CFRunLoopRun();
            }
        });
    }

    extern "C" fn tap_callback(
        _proxy: *mut c_void,
        type_: u32,
        event: *mut c_void,
        refcon: *mut c_void,
    ) -> *mut c_void {
        if type_ == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT {
            unsafe {
                if !refcon.is_null() {
                    let ctx = &*(refcon as *mut TapContext);
                    let port = ctx.tap_port.load(Ordering::SeqCst);
                    if !port.is_null() {
                        CGEventTapEnable(port, true);
                    }
                }
            }
            return event;
        }

        if type_ != K_CG_EVENT_KEY_DOWN || refcon.is_null() || event.is_null() {
            return event;
        }

        let ctx = unsafe { &*(refcon as *mut TapContext) };
        let flags = unsafe { CGEventGetFlags(event) };
        let keycode = unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) };

        let has_cmd = (flags & MASK_CMD) != 0;
        let has_ctrl = (flags & MASK_CTRL) != 0;
        let has_shift = (flags & MASK_SHIFT) != 0;
        let ctrl_or_cmd = has_cmd || has_ctrl;

        let (double_tap_enabled, double_tap_timeout, slot_count, auto_paste_pop, auto_cut_fallback) = {
            let s = ctx.settings.read().unwrap();
            (
                s.double_tap_enabled,
                s.double_tap_timeout_ms,
                s.slot_count,
                s.auto_paste_pop,
                s.auto_cut_fallback,
            )
        };

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // 0. Toggle HUD Shortcut: Cmd + Shift + Space
        if has_cmd && has_shift && keycode == VK_SPACE {
            (ctx.event_sink)(EngineEvent::ToggleHUD);
            return std::ptr::null_mut();
        }

        // 1. Detect Copy Mode: Ctrl/Cmd + Shift + C or Double-tap C
        if ctrl_or_cmd && has_shift && keycode == VK_ANSI_C {
            let last = ctx.last_c_tap_ms.load(Ordering::SeqCst);
            let is_double_tap = double_tap_enabled && now_ms.saturating_sub(last) < double_tap_timeout;
            ctx.last_c_tap_ms.store(now_ms, Ordering::SeqCst);

            let current_copy = ctx.is_copy_mode.load(Ordering::SeqCst);
            if current_copy {
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

            if is_double_tap {
                return std::ptr::null_mut();
            }
            return event;
        }

        // 2. Detect Manual Cut Mode: Ctrl/Cmd + Shift + X or Double-tap X
        if ctrl_or_cmd && has_shift && keycode == VK_ANSI_X {
            let last = ctx.last_x_tap_ms.load(Ordering::SeqCst);
            let is_double_tap = double_tap_enabled && now_ms.saturating_sub(last) < double_tap_timeout;
            ctx.last_x_tap_ms.store(now_ms, Ordering::SeqCst);

            let current_cut = ctx.is_cut_mode.load(Ordering::SeqCst);
            if current_cut {
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

            if is_double_tap {
                return std::ptr::null_mut();
            }
            return event;
        }

        // 3. Detect Paste Mode: Ctrl/Cmd + Shift + V or Double-tap V
        if ctrl_or_cmd && has_shift && keycode == VK_ANSI_V {
            let last = ctx.last_v_tap_ms.load(Ordering::SeqCst);
            let is_double_tap = double_tap_enabled && now_ms.saturating_sub(last) < double_tap_timeout;
            ctx.last_v_tap_ms.store(now_ms, Ordering::SeqCst);

            let current_paste = ctx.is_paste_mode.load(Ordering::SeqCst);
            if current_paste {
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
                    info: format!("PASTE MODE: Tap 1-{} to paste (Esc to exit)", slot_count),
                });
            }

            if is_double_tap {
                return std::ptr::null_mut();
            }
            return event;
        }

        // 4. In Copy Mode: User hits 1..slot_count
        if ctx.is_copy_mode.load(Ordering::SeqCst) {
            if keycode == VK_ESCAPE {
                ctx.is_copy_mode.store(false, Ordering::SeqCst);
                (ctx.event_sink)(EngineEvent::ModeChanged {
                    mode: "idle".into(),
                    info: "AUTO MODE".into(),
                });
                return std::ptr::null_mut();
            }

            if let Some(digit) = keycode_to_digit(keycode) {
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
                                let mut manager = sm.lock().unwrap();
                                manager.assign(slot_idx, current);
                            }
                            (sink)(EngineEvent::SlotsUpdated);
                            (sink)(EngineEvent::ModeChanged {
                                mode: "copy".into(),
                                info: format!("Saved to Slot {}!", digit),
                            });
                        }
                    });

                    return std::ptr::null_mut();
                }
            }
        }

        // 5. In Manual Cut Mode: User hits 1..slot_count
        if ctx.is_cut_mode.load(Ordering::SeqCst) {
            if keycode == VK_ESCAPE {
                ctx.is_cut_mode.store(false, Ordering::SeqCst);
                (ctx.event_sink)(EngineEvent::ModeChanged {
                    mode: "idle".into(),
                    info: "AUTO MODE".into(),
                });
                return std::ptr::null_mut();
            }

            if let Some(digit) = keycode_to_digit(keycode) {
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
                        let cut_succeeded = after_cut.is_some() && after_cut != before;

                        let (final_content, was_cut) = if cut_succeeded {
                            (after_cut, true)
                        } else if auto_cut_fallback {
                            simulate::simulate_copy();
                            thread::sleep(Duration::from_millis(75));
                            (cs.read_current(), false)
                        } else {
                            (None, false)
                        };

                        if let Some(content) = final_content {
                            {
                                let mut manager = sm.lock().unwrap();
                                manager.assign(slot_idx, content);
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

                    return std::ptr::null_mut();
                }
            }
        }

        // 6. In Paste Mode: User hits 1..slot_count
        if ctx.is_paste_mode.load(Ordering::SeqCst) {
            if keycode == VK_ESCAPE {
                ctx.is_paste_mode.store(false, Ordering::SeqCst);
                (ctx.event_sink)(EngineEvent::ModeChanged {
                    mode: "idle".into(),
                    info: "AUTO MODE".into(),
                });
                return std::ptr::null_mut();
            }

            if let Some(digit) = keycode_to_digit(keycode) {
                if digit <= slot_count {
                    let slot_idx = digit - 1;
                    let sm = ctx.slot_manager.clone();
                    let cs = ctx.clipboard_service.clone();

                    thread::spawn(move || {
                        let item = {
                            let manager = sm.lock().unwrap();
                            manager.get(slot_idx)
                        };

                        if let Some(item) = item {
                            let _ = cs.write_content(&item.content);
                            simulate::simulate_paste();
                        }
                    });

                    return std::ptr::null_mut();
                }
            }
        }

        // 7. Auto Mode Cmd+V / Cmd+X
        if !ctx.is_copy_mode.load(Ordering::SeqCst)
            && !ctx.is_cut_mode.load(Ordering::SeqCst)
            && !ctx.is_paste_mode.load(Ordering::SeqCst)
        {
            if has_cmd && !has_shift && keycode == VK_ANSI_V {
                let was_explicit = ctx.is_explicit_selection.swap(false, Ordering::SeqCst);

                if !was_explicit {
                    let has_items = {
                        let manager = ctx.slot_manager.lock().unwrap();
                        manager.has_auto_items()
                    };

                    if has_items {
                        let popped = {
                            let mut manager = ctx.slot_manager.lock().unwrap();
                            manager.auto_paste_pop_custom(auto_paste_pop)
                        };

                        if let Some(item) = popped {
                            let _ = ctx.clipboard_service.write_content(&item);
                            (ctx.event_sink)(EngineEvent::SlotsUpdated);
                        }
                    }
                }
            }

            if has_cmd && !has_shift && keycode == VK_ANSI_X {
                let cs = ctx.clipboard_service.clone();

                thread::spawn(move || {
                    let before = cs.read_current();
                    thread::sleep(Duration::from_millis(80));
                    let after = cs.read_current();

                    if after == before && auto_cut_fallback {
                        simulate::simulate_copy();
                    }
                });
            }
        }

        event
    }

    fn keycode_to_digit(code: i64) -> Option<usize> {
        match code {
            VK_ANSI_1 => Some(1),
            VK_ANSI_2 => Some(2),
            VK_ANSI_3 => Some(3),
            VK_ANSI_4 => Some(4),
            VK_ANSI_5 => Some(5),
            VK_ANSI_6 => Some(6),
            VK_ANSI_7 => Some(7),
            VK_ANSI_8 => Some(8),
            VK_ANSI_9 => Some(9),
            VK_ANSI_0 => Some(10),
            _ => None,
        }
    }
}
