#[cfg(not(target_os = "windows"))]
use std::thread;
#[cfg(not(target_os = "windows"))]
use std::time::Duration;

pub fn simulate_paste() {
    #[cfg(target_os = "macos")]
    {
        simulate_macos_key(rdev::Key::KeyV);
    }

    #[cfg(target_os = "windows")]
    {
        simulate_windows_ctrl_key(0x56); // VK_V
    }

    #[cfg(target_os = "linux")]
    {
        simulate_rdev_ctrl_key(rdev::Key::KeyV);
    }
}

pub fn simulate_copy() {
    #[cfg(target_os = "macos")]
    {
        simulate_macos_key(rdev::Key::KeyC);
    }

    #[cfg(target_os = "windows")]
    {
        simulate_windows_ctrl_key(0x43); // VK_C
    }

    #[cfg(target_os = "linux")]
    {
        simulate_rdev_ctrl_key(rdev::Key::KeyC);
    }
}

pub fn simulate_cut() {
    #[cfg(target_os = "macos")]
    {
        simulate_macos_key(rdev::Key::KeyX);
    }

    #[cfg(target_os = "windows")]
    {
        simulate_windows_ctrl_key(0x58); // VK_X
    }

    #[cfg(target_os = "linux")]
    {
        simulate_rdev_ctrl_key(rdev::Key::KeyX);
    }
}

#[cfg(target_os = "macos")]
fn simulate_macos_key(key: rdev::Key) {
    let _ = rdev::simulate(&rdev::EventType::KeyPress(rdev::Key::MetaLeft));
    thread::sleep(Duration::from_millis(15));
    let _ = rdev::simulate(&rdev::EventType::KeyPress(key));
    thread::sleep(Duration::from_millis(20));
    let _ = rdev::simulate(&rdev::EventType::KeyRelease(key));
    thread::sleep(Duration::from_millis(15));
    let _ = rdev::simulate(&rdev::EventType::KeyRelease(rdev::Key::MetaLeft));
}

#[cfg(target_os = "windows")]
fn simulate_windows_ctrl_key(vk: u16) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_CONTROL,
    };

    unsafe {
        let mut inputs = [
            // Ctrl Down
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: 0,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Key Down
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: 0,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Key Up
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Ctrl Up
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

#[cfg(target_os = "linux")]
fn simulate_rdev_ctrl_key(key: rdev::Key) {
    let _ = rdev::simulate(&rdev::EventType::KeyPress(rdev::Key::ControlLeft));
    thread::sleep(Duration::from_millis(15));
    let _ = rdev::simulate(&rdev::EventType::KeyPress(key));
    thread::sleep(Duration::from_millis(20));
    let _ = rdev::simulate(&rdev::EventType::KeyRelease(key));
    thread::sleep(Duration::from_millis(15));
    let _ = rdev::simulate(&rdev::EventType::KeyRelease(rdev::Key::ControlLeft));
}
