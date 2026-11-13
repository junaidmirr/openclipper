pub mod linux;
pub mod macos;
pub mod simulate;
pub mod windows;

use crate::clipboard::ClipboardService;
use crate::settings::SharedSettings;
use crate::slots::SharedSlotManager;
use crate::EngineEvent;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub fn start_input_hook(
    event_sink: Arc<dyn Fn(EngineEvent) + Send + Sync>,
    slot_manager: SharedSlotManager,
    clipboard_service: Arc<ClipboardService>,
    settings: SharedSettings,
    is_copy_mode: Arc<AtomicBool>,
    is_cut_mode: Arc<AtomicBool>,
    is_paste_mode: Arc<AtomicBool>,
    is_explicit_selection: Arc<AtomicBool>,
) {
    #[cfg(target_os = "macos")]
    {
        macos::tap::start_tap(
            event_sink,
            slot_manager,
            clipboard_service,
            settings,
            is_copy_mode,
            is_cut_mode,
            is_paste_mode,
            is_explicit_selection,
        );
    }

    #[cfg(target_os = "windows")]
    {
        windows::hook::start_hook(
            event_sink,
            slot_manager,
            clipboard_service,
            settings,
            is_copy_mode,
            is_cut_mode,
            is_paste_mode,
            is_explicit_selection,
        );
    }

    #[cfg(target_os = "linux")]
    {
        linux::linux_hook::start_hook(
            event_sink,
            slot_manager,
            clipboard_service,
            settings,
            is_copy_mode,
            is_cut_mode,
            is_paste_mode,
            is_explicit_selection,
        );
    }
}

pub fn is_accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::tap::is_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub fn prompt_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::tap::prompt_accessibility()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}
