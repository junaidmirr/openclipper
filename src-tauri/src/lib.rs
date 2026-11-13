use openclipper_engine::settings::AppSettings;
use openclipper_engine::slots::SlotItem;
use openclipper_engine::{Engine, EngineEvent};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct AppState {
    pub engine: Arc<Engine>,
}

#[derive(serde::Serialize, Clone)]
struct SlotsChangedPayload {
    slots: Vec<Option<SlotItem>>,
    auto_count: usize,
    slot_count: usize,
}

#[derive(serde::Serialize, Clone)]
struct ModeChangedPayload {
    mode: String,
    info: String,
}

#[tauri::command]
fn get_slots(state: State<'_, AppState>) -> serde_json::Value {
    let (slots, auto_count, capacity) = state.engine.get_slots();
    serde_json::json!({
        "slots": slots,
        "auto_count": auto_count,
        "slot_count": capacity,
        "is_paste": state.engine.is_paste_mode.load(std::sync::atomic::Ordering::SeqCst),
        "is_copy": state.engine.is_copy_mode.load(std::sync::atomic::Ordering::SeqCst),
        "is_cut": state.engine.is_cut_mode.load(std::sync::atomic::Ordering::SeqCst),
    })
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.engine.get_settings()
}

#[tauri::command]
fn update_settings(
    new_settings: AppSettings,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.engine.update_settings(new_settings.clone())?;
    emit_slots_updated(&app, &state.engine);
    let _ = app.emit("settings-changed", &new_settings);
    Ok(())
}

#[tauri::command]
fn reset_settings(app: AppHandle, state: State<'_, AppState>) -> Result<AppSettings, String> {
    let defaults = state.engine.reset_settings()?;
    emit_slots_updated(&app, &state.engine);
    let _ = app.emit("settings-changed", &defaults);
    Ok(defaults)
}

#[tauri::command]
fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
    }
    Ok(())
}

#[tauri::command]
fn close_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.hide();
    }
    Ok(())
}

#[tauri::command]
fn clear_all_slots(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.engine.clear_all_slots();
    emit_slots_updated(&app, &state.engine);
    Ok(())
}

#[tauri::command]
fn select_slot(
    slot_index: usize,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.engine.select_slot(slot_index)
}

#[tauri::command]
fn assign_to_slot(
    slot_index: usize,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.engine.assign_to_slot(slot_index)
}

#[tauri::command]
fn set_app_mode(
    new_mode: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.engine.set_mode(&new_mode);
    Ok(())
}

#[tauri::command]
fn start_dragging(window: tauri::WebviewWindow) -> Result<(), String> {
    let _ = window.start_dragging();
    Ok(())
}

#[tauri::command]
fn check_accessibility(state: State<'_, AppState>) -> bool {
    state.engine.check_accessibility()
}

#[tauri::command]
fn prompt_accessibility(state: State<'_, AppState>) -> bool {
    state.engine.prompt_accessibility()
}

#[tauri::command]
fn dismiss_hud(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.engine.set_mode("idle");
    hide_hud_window(&app);
    let _ = app.emit(
        "mode-changed",
        ModeChangedPayload {
            mode: "idle".into(),
            info: "".into(),
        },
    );
    Ok(())
}

#[tauri::command]
fn show_hud(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    show_hud_top_right(&app);
    let mode = if state.engine.is_paste_mode.load(std::sync::atomic::Ordering::SeqCst) {
        "paste"
    } else if state.engine.is_cut_mode.load(std::sync::atomic::Ordering::SeqCst) {
        "cut"
    } else if state.engine.is_copy_mode.load(std::sync::atomic::Ordering::SeqCst) {
        "copy"
    } else {
        "idle"
    };

    let _ = app.emit(
        "mode-changed",
        ModeChangedPayload {
            mode: mode.into(),
            info: "".into(),
        },
    );
    Ok(())
}

pub fn emit_slots_updated(app: &AppHandle, engine: &Engine) {
    let (slots, auto_count, slot_count) = engine.get_slots();
    let _ = app.emit(
        "slots-changed",
        SlotsChangedPayload {
            slots,
            auto_count,
            slot_count,
        },
    );
}

pub fn show_hud_top_right(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(Some(monitor)) = window.current_monitor() {
            let screen_size = monitor.size();
            let scale = monitor.scale_factor();
            let win_width = (480.0 * scale) as i32;
            let x = screen_size.width as i32 - win_width - 16;
            let y = (36.0 * scale) as i32;

            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
        }
        let _ = window.show();
    }
}

pub fn hide_hud_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

pub fn toggle_hud(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            hide_hud_window(app);
        } else {
            show_hud_top_right(app);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle_events = app.handle().clone();

            // Initialize OpenClipper Headless Cross-Platform Engine
            let engine = Arc::new(Engine::new(move |event| match event {
                EngineEvent::SlotsUpdated => {
                    if let Some(state) = app_handle_events.try_state::<AppState>() {
                        emit_slots_updated(&app_handle_events, &state.engine);
                    }
                }
                EngineEvent::SlotSelected { slot_index } => {
                    let _ = app_handle_events.emit(
                        "slot-selected",
                        serde_json::json!({ "slotIndex": slot_index }),
                    );
                }
                EngineEvent::ModeChanged { mode, info } => {
                    let _ = app_handle_events.emit("mode-changed", ModeChangedPayload { mode, info });
                }
                EngineEvent::AccessibilityStatus(trusted) => {
                    let _ = app_handle_events.emit("accessibility-status", trusted);
                }
                EngineEvent::ToggleHUD => {
                    toggle_hud(&app_handle_events);
                }
                EngineEvent::ShowHUD => {
                    show_hud_top_right(&app_handle_events);
                }
                EngineEvent::HideHUD => {
                    hide_hud_window(&app_handle_events);
                }
            })?);

            app.manage(AppState {
                engine: engine.clone(),
            });

            // Start background watchers and native hooks
            engine.start();

            // Menu Bar Tray Icon
            let toggle_item = MenuItem::with_id(
                app,
                "toggle",
                "Toggle OpenClipper HUD",
                true,
                None::<&str>,
            )?;
            let mode_auto = MenuItem::with_id(
                app,
                "mode_auto",
                "Mode: Auto Conveyor",
                true,
                None::<&str>,
            )?;
            let mode_copy = MenuItem::with_id(
                app,
                "mode_copy",
                "Mode: Manual Copy",
                true,
                None::<&str>,
            )?;
            let mode_cut = MenuItem::with_id(
                app,
                "mode_cut",
                "Mode: Manual Cut",
                true,
                None::<&str>,
            )?;
            let mode_paste = MenuItem::with_id(
                app,
                "mode_paste",
                "Mode: Manual Paste",
                true,
                None::<&str>,
            )?;
            let clear_item = MenuItem::with_id(app, "clear", "Clear All Slots", true, None::<&str>)?;
            let settings_item =
                MenuItem::with_id(app, "settings", "Preferences...", true, Some("CmdOrCtrl+,"))?;
            let quit_item =
                MenuItem::with_id(app, "quit", "Quit OpenClipper", true, Some("CmdOrCtrl+Q"))?;
            let menu = Menu::with_items(
                app,
                &[
                    &toggle_item,
                    &mode_auto,
                    &mode_copy,
                    &mode_cut,
                    &mode_paste,
                    &clear_item,
                    &settings_item,
                    &quit_item,
                ],
            )?;

            let engine_tray = engine.clone();
            let icon_img = app.default_window_icon().cloned().expect("Default window icon");

            let _tray = TrayIconBuilder::new()
                .icon(icon_img)
                .menu(&menu)
                .tooltip("OpenClipper - Modal Clipboard")
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "toggle" => {
                        toggle_hud(app);
                    }
                    "mode_auto" => {
                        engine_tray.set_mode("idle");
                    }
                    "mode_copy" => {
                        engine_tray.set_mode("copy");
                        show_hud_top_right(app);
                    }
                    "mode_cut" => {
                        engine_tray.set_mode("cut");
                        show_hud_top_right(app);
                    }
                    "mode_paste" => {
                        engine_tray.set_mode("paste");
                        show_hud_top_right(app);
                    }
                    "clear" => {
                        engine_tray.clear_all_slots();
                        emit_slots_updated(app, &engine_tray);
                    }
                    "settings" => {
                        if let Some(win) = app.get_webview_window("settings") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(move |tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        toggle_hud(app);
                    }
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_slots,
            get_settings,
            update_settings,
            reset_settings,
            open_settings_window,
            close_settings_window,
            select_slot,
            assign_to_slot,
            clear_all_slots,
            set_app_mode,
            start_dragging,
            check_accessibility,
            prompt_accessibility,
            dismiss_hud,
            show_hud
        ])
        .run(tauri::generate_context!())
        .expect("error while running openclipper application");
}
