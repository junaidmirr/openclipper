pub mod linux;
pub mod macos;
pub mod windows;

use crate::slots::SlotContent;
use arboard::{Clipboard, ImageData};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::{ImageBuffer, Rgba};
use std::borrow::Cow;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

pub struct ClipboardService {
    clipboard: Arc<Mutex<Clipboard>>,
    last_written: Arc<Mutex<Option<SlotContent>>>,
}

impl ClipboardService {
    pub fn new() -> Result<Self, String> {
        let cb = Clipboard::new().map_err(|e| format!("Failed to init clipboard: {e}"))?;
        Ok(Self {
            clipboard: Arc::new(Mutex::new(cb)),
            last_written: Arc::new(Mutex::new(None)),
        })
    }

    /// Read the current clipboard payload (Files, Image, or Text) with panic protection
    pub fn read_current(&self) -> Option<SlotContent> {
        let res =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.read_current_inner()));

        match res {
            Ok(content) => content,
            Err(_) => {
                eprintln!("[OpenClipper] Warning: caught panic reading clipboard");
                None
            }
        }
    }

    fn read_current_inner(&self) -> Option<SlotContent> {
        // 1. Check for files (platform specific)
        #[cfg(target_os = "macos")]
        {
            if let Some(files) = macos::macos_files::read_files_from_pasteboard() {
                if !files.is_empty() {
                    return Some(SlotContent::Files { files });
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(files) = windows::windows_files::read_files_from_clipboard() {
                if !files.is_empty() {
                    return Some(SlotContent::Files { files });
                }
            }
        }

        let mut cb = self.clipboard.lock().ok()?;

        // 2. Try Image
        if let Ok(img_data) = cb.get_image() {
            if let Some(content) = process_image_data(img_data) {
                return Some(content);
            }
        }

        // 3. Try Text
        if let Ok(text) = cb.get_text() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                // On Linux, text might also be a file uri-list
                #[cfg(target_os = "linux")]
                {
                    if let Some(files) = linux::linux_files::parse_uri_list(&text) {
                        return Some(SlotContent::Files { files });
                    }
                }

                return Some(SlotContent::Text {
                    text: text.to_string(),
                });
            }
        }

        None
    }

    /// Read clipboard, but ignore changes that were written by OpenClipper itself (prevents duplicate loop)
    pub fn read_external(&self) -> Option<SlotContent> {
        let current = self.read_current()?;

        let mut last = self.last_written.lock().unwrap();
        if let Some(ref written) = *last {
            if written == &current {
                // Change originated from OpenClipper internally; ignore it
                return None;
            }
        }

        // An external copy occurred; reset last_written
        *last = None;
        Some(current)
    }

    /// Restore a slot's content back into the OS clipboard
    pub fn write_content(&self, content: &SlotContent) -> Result<(), String> {
        // Record what we are writing so the watcher doesn't treat it as a new copy
        {
            let mut last = self.last_written.lock().unwrap();
            *last = Some(content.clone());
        }

        let mut cb = self.clipboard.lock().map_err(|e| e.to_string())?;

        match content {
            SlotContent::Text { text } => {
                cb.set_text(text.clone()).map_err(|e| e.to_string())?;
            }
            SlotContent::Image {
                width,
                height,
                full_bytes,
                ..
            } => {
                if let Some(bytes) = full_bytes {
                    let img_data = ImageData {
                        width: *width as usize,
                        height: *height as usize,
                        bytes: Cow::Borrowed(bytes),
                    };
                    cb.set_image(img_data).map_err(|e| e.to_string())?;
                }
            }
            SlotContent::Files { files } => {
                #[cfg(target_os = "macos")]
                {
                    macos::macos_files::write_files_to_pasteboard(files)?;
                }
                #[cfg(target_os = "windows")]
                {
                    windows::windows_files::write_files_to_clipboard(files)?;
                }
                #[cfg(target_os = "linux")]
                {
                    let uri_list = files
                        .iter()
                        .map(|f| format!("file://{}", f))
                        .collect::<Vec<_>>()
                        .join("\r\n");
                    let _ = cb.set_text(uri_list);
                }
            }
        }

        Ok(())
    }
}

/// Process image safely: creates micro thumbnail for UI and stores raw bytes in memory
fn process_image_data(img: ImageData<'_>) -> Option<SlotContent> {
    let width = img.width as u32;
    let height = img.height as u32;

    if width == 0 || height == 0 || img.bytes.is_empty() {
        return None;
    }

    let full_raw_bytes = img.bytes.to_vec();

    let thumb_max = 64u32;
    let (t_w, t_h) = if width > thumb_max || height > thumb_max {
        if width >= height {
            let ratio = thumb_max as f32 / width as f32;
            (thumb_max, ((height as f32 * ratio) as u32).max(1))
        } else {
            let ratio = thumb_max as f32 / height as f32;
            (((width as f32 * ratio) as u32).max(1), thumb_max)
        }
    } else {
        (width, height)
    };

    let mut thumb_buf: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(t_w, t_h);
    let x_ratio = (width as f32) / (t_w as f32);
    let y_ratio = (height as f32) / (t_h as f32);

    for ty in 0..t_h {
        let src_y = ((ty as f32 * y_ratio) as usize).min((height - 1) as usize);
        for tx in 0..t_w {
            let src_x = ((tx as f32 * x_ratio) as usize).min((width - 1) as usize);
            let idx = (src_y * (width as usize) + src_x) * 4;
            if idx + 3 < full_raw_bytes.len() {
                thumb_buf.put_pixel(
                    tx,
                    ty,
                    Rgba([
                        full_raw_bytes[idx],
                        full_raw_bytes[idx + 1],
                        full_raw_bytes[idx + 2],
                        full_raw_bytes[idx + 3],
                    ]),
                );
            }
        }
    }

    let mut png_bytes = Cursor::new(Vec::new());
    if thumb_buf
        .write_to(&mut png_bytes, image::ImageFormat::Png)
        .is_err()
    {
        return None;
    }

    let b64 = BASE64.encode(png_bytes.into_inner());
    let data_url = format!("data:image/png;base64,{b64}");

    Some(SlotContent::Image {
        data_url,
        width,
        height,
        full_bytes: Some(full_raw_bytes),
    })
}
