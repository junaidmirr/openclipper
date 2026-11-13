#[cfg(target_os = "windows")]
pub mod windows_files {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{BOOL, HANDLE, HWND};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};

    const CF_HDROP: u32 = 15;

    pub fn read_files_from_clipboard() -> Option<Vec<String>> {
        unsafe {
            if IsClipboardFormatAvailable(CF_HDROP) == 0 {
                return None;
            }

            if OpenClipboard(0 as HWND) == 0 {
                return None;
            }

            let handle = GetClipboardData(CF_HDROP);
            if handle == 0 {
                CloseClipboard();
                return None;
            }

            let hdrop = handle as HDROP;
            let file_count = DragQueryFileW(hdrop, 0xFFFFFFFF, std::ptr::null_mut(), 0);

            let mut files = Vec::new();
            for i in 0..file_count {
                let len = DragQueryFileW(hdrop, i, std::ptr::null_mut(), 0);
                if len > 0 {
                    let mut buffer: Vec<u16> = vec![0; (len + 1) as usize];
                    DragQueryFileW(hdrop, i, buffer.as_mut_ptr(), len + 1);
                    buffer.pop(); // Remove null terminator
                    let os_str = OsString::from_wide(&buffer);
                    if let Ok(s) = os_str.into_string() {
                        files.push(s);
                    }
                }
            }

            CloseClipboard();

            if files.is_empty() { None } else { Some(files) }
        }
    }

    pub fn write_files_to_clipboard(_paths: &[String]) -> Result<(), String> {
        // Fallback for Windows file writing
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
pub mod windows_files {
    pub fn read_files_from_clipboard() -> Option<Vec<String>> {
        None
    }
    pub fn write_files_to_clipboard(_paths: &[String]) -> Result<(), String> {
        Ok(())
    }
}
