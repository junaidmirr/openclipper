use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    // Shortcuts
    pub manual_copy_shortcut: String,  // e.g. "Ctrl+Shift+C"
    pub manual_cut_shortcut: String,   // e.g. "Ctrl+Shift+X"
    pub manual_paste_shortcut: String, // e.g. "Ctrl+Shift+V"
    pub toggle_hud_shortcut: String,   // e.g. "Cmd+Shift+Space" / "Ctrl+Shift+Space"
    pub double_tap_enabled: bool,      // default: true
    pub double_tap_timeout_ms: u64,    // default: 500

    // Slot Behavior
    pub slot_count: usize,           // default: 9 (range: 3..15)
    pub auto_paste_pop: bool,        // default: true (conveyor belt pop)
    pub auto_copy_direction: String, // "fifo" (append) or "lifo" (prepend)
    pub auto_cut_fallback: bool,     // default: true
    pub persist_slots: bool,         // default: false
    pub play_sounds: bool,           // default: true

    // Appearance & Themes
    pub theme: String,          // "light" | "dark" | "system"
    pub accent_color: String,   // "orange" | "blue" | "green" | "purple" | "red"
    pub hud_opacity: u32,       // default: 100
    pub show_hud_on_copy: bool, // default: false

    // General & System
    pub launch_at_startup: bool, // default: false
}

impl Default for AppSettings {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        let toggle = "Cmd+Shift+Space";
        #[cfg(not(target_os = "macos"))]
        let toggle = "Ctrl+Shift+Space";

        Self {
            manual_copy_shortcut: "Ctrl+Shift+C".into(),
            manual_cut_shortcut: "Ctrl+Shift+X".into(),
            manual_paste_shortcut: "Ctrl+Shift+V".into(),
            toggle_hud_shortcut: toggle.into(),
            double_tap_enabled: true,
            double_tap_timeout_ms: 500,

            slot_count: 9,
            auto_paste_pop: true,
            auto_copy_direction: "fifo".into(),
            auto_cut_fallback: true,
            persist_slots: false,
            play_sounds: true,

            theme: "light".into(),
            accent_color: "orange".into(),
            hud_opacity: 100,
            show_hud_on_copy: false,

            launch_at_startup: false,
        }
    }
}

pub type SharedSettings = Arc<RwLock<AppSettings>>;

impl AppSettings {
    pub fn config_dir() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                PathBuf::from(appdata).join("openclipper")
            } else {
                PathBuf::from("openclipper_config")
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            if let Ok(home) = std::env::var("HOME") {
                PathBuf::from(home).join(".config").join("openclipper")
            } else {
                PathBuf::from("openclipper_config")
            }
        }
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("settings.json")
    }

    pub fn slots_storage_path() -> PathBuf {
        Self::config_dir().join("slots.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(parsed) = serde_json::from_str::<AppSettings>(&content) {
                    return parsed;
                }
            }
        }
        let default_settings = Self::default();
        let _ = default_settings.save();
        default_settings
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())?;

        // Apply autostart for current OS
        let _ = set_launch_at_startup(self.launch_at_startup);

        Ok(())
    }
}

pub fn set_launch_at_startup(enable: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let plist_path = PathBuf::from(home)
                .join("Library")
                .join("LaunchAgents")
                .join("com.openclipper.app.plist");

            if enable {
                if let Ok(exe_path) = std::env::current_exe() {
                    if let Some(parent) = plist_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let plist_content = format!(
                        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.openclipper.app</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#,
                        exe_path.display()
                    );
                    let _ = fs::write(plist_path, plist_content);
                }
            } else if plist_path.exists() {
                let _ = fs::remove_file(plist_path);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let autostart_dir = PathBuf::from(home).join(".config").join("autostart");
            let desktop_path = autostart_dir.join("openclipper.desktop");

            if enable {
                if let Ok(exe_path) = std::env::current_exe() {
                    let _ = fs::create_dir_all(&autostart_dir);
                    let desktop_entry = format!(
                        r#"[Desktop Entry]
Type=Application
Name=OpenClipper
Exec={}
Terminal=false
Categories=Utility;
"#,
                        exe_path.display()
                    );
                    let _ = fs::write(desktop_path, desktop_entry);
                }
            } else if desktop_path.exists() {
                let _ = fs::remove_file(desktop_path);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows, places shortcut or .bat in user Startup folder
        if let Ok(appdata) = std::env::var("APPDATA") {
            let startup_dir = PathBuf::from(appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup");
            let cmd_path = startup_dir.join("openclipper.cmd");

            if enable {
                if let Ok(exe_path) = std::env::current_exe() {
                    let _ = fs::create_dir_all(&startup_dir);
                    let cmd_content = format!("start \"\" \"{}\"\r\n", exe_path.display());
                    let _ = fs::write(cmd_path, cmd_content);
                }
            } else if cmd_path.exists() {
                let _ = fs::remove_file(cmd_path);
            }
        }
    }

    Ok(())
}
